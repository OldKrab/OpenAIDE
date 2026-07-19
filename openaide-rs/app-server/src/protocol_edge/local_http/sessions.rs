use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex, Weak};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::client_lifecycle::ConnectionId;

pub(super) const MAX_SERVER_REPLAY_FRAMES: usize = 1_024;
pub(super) const MAX_ACTIVE_RELIABLE_SESSIONS: usize = 1_024;
const DEFAULT_RELIABLE_SESSION_IDLE_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone)]
pub(super) struct ReliableSessionRegistry {
    server_id: String,
    sessions: Arc<Mutex<HashMap<String, ReliableSession>>>,
    idle_timeout: Duration,
    _reaper: Arc<ReliableSessionReaper>,
}

#[derive(Debug)]
struct ReliableSessionReaper {
    shutdown: mpsc::Sender<()>,
    worker: Mutex<Option<JoinHandle<()>>>,
    stopped: Arc<AtomicBool>,
}

#[derive(Debug)]
struct ReliableSession {
    connection_id: ConnectionId,
    last_client_sequence: u64,
    next_server_sequence: u64,
    server_frames: VecDeque<ServerFrame>,
    last_activity: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OpenedSession {
    pub session_id: String,
    pub server_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AcceptClientFrame {
    Accepted,
    Duplicate,
    Gap { expected: u64 },
    UnknownSession,
    WrongConnection,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ServerFrame {
    pub sequence: u64,
    pub message: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ServerBatch {
    pub frames: Vec<ServerFrame>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PollError {
    UnknownSession,
    InvalidAcknowledgement,
    ReplayExpired,
}

impl ReliableSessionRegistry {
    pub fn new(server_id: impl Into<String>) -> Self {
        Self::with_timeout(server_id, DEFAULT_RELIABLE_SESSION_IDLE_TIMEOUT)
    }

    #[cfg(test)]
    pub fn with_idle_timeout(server_id: impl Into<String>, idle_timeout: Duration) -> Self {
        Self::with_timeout(server_id, idle_timeout)
    }

    fn with_timeout(server_id: impl Into<String>, idle_timeout: Duration) -> Self {
        let sessions = Arc::new(Mutex::new(HashMap::new()));
        Self {
            server_id: server_id.into(),
            _reaper: Arc::new(ReliableSessionReaper::start(
                Arc::downgrade(&sessions),
                idle_timeout,
            )),
            sessions,
            idle_timeout,
        }
    }

    pub fn open(&self, connection_id: ConnectionId) -> OpenedSession {
        let session_id = Uuid::new_v4().to_string();
        let now = Instant::now();
        let mut sessions = self.sessions.lock().expect("session registry poisoned");
        sessions.retain(|_, session| now.duration_since(session.last_activity) < self.idle_timeout);
        while sessions.len() >= MAX_ACTIVE_RELIABLE_SESSIONS {
            let oldest = sessions
                .iter()
                .min_by_key(|(_, session)| session.last_activity)
                .map(|(session_id, _)| session_id.clone());
            let Some(oldest) = oldest else { break };
            sessions.remove(&oldest);
        }
        sessions.insert(
            session_id.clone(),
            ReliableSession {
                connection_id,
                last_client_sequence: 0,
                next_server_sequence: 1,
                server_frames: VecDeque::new(),
                last_activity: now,
            },
        );
        OpenedSession {
            session_id,
            server_id: self.server_id.clone(),
        }
    }

    /// Accepts a sequenced frame exactly once within one App Server generation.
    pub fn accept_client_frame(
        &self,
        session_id: &str,
        connection_id: &ConnectionId,
        sequence: u64,
        message: Value,
        dispatch: impl FnOnce(Value),
    ) -> AcceptClientFrame {
        let mut sessions = self.sessions.lock().expect("session registry poisoned");
        let Some(session) = sessions.get_mut(session_id) else {
            return AcceptClientFrame::UnknownSession;
        };
        if &session.connection_id != connection_id {
            return AcceptClientFrame::WrongConnection;
        }
        session.last_activity = Instant::now();
        if sequence <= session.last_client_sequence {
            return AcceptClientFrame::Duplicate;
        }
        let expected = session.last_client_sequence + 1;
        if sequence != expected {
            return AcceptClientFrame::Gap { expected };
        }
        // Advance before dispatch so a transport retry cannot invoke the handler
        // twice even when the acknowledgement is lost after dispatch completes.
        session.last_client_sequence = sequence;
        drop(sessions);
        dispatch(message);
        AcceptClientFrame::Accepted
    }

    pub fn enqueue_server_message(&self, session_id: &str, message: Value) -> bool {
        let mut sessions = self.sessions.lock().expect("session registry poisoned");
        let Some(session) = sessions.get_mut(session_id) else {
            return false;
        };
        session.last_activity = Instant::now();
        let sequence = session.next_server_sequence;
        session.next_server_sequence += 1;
        session
            .server_frames
            .push_back(ServerFrame { sequence, message });
        if session.server_frames.len() > MAX_SERVER_REPLAY_FRAMES {
            session.server_frames.pop_front();
        }
        true
    }

    pub fn connection_id(&self, session_id: &str) -> Option<ConnectionId> {
        let now = Instant::now();
        let mut sessions = self.sessions.lock().expect("session registry poisoned");
        // Every reliable poll resolves its connection through this lookup, so an
        // active replacement generation also sweeps abandoned close handshakes.
        sessions.retain(|_, session| now.duration_since(session.last_activity) < self.idle_timeout);
        sessions
            .get(session_id)
            .map(|session| session.connection_id.clone())
    }

    #[cfg(test)]
    fn session_count(&self) -> usize {
        self.sessions
            .lock()
            .expect("session registry poisoned")
            .len()
    }

    #[cfg(test)]
    fn reaper_stopped(&self) -> Arc<AtomicBool> {
        self._reaper.stopped.clone()
    }

    /// Removes only the reliable session owned by this physical connection generation.
    pub fn close(&self, session_id: &str, connection_id: &ConnectionId) -> bool {
        let mut sessions = self.sessions.lock().expect("session registry poisoned");
        let matches = sessions
            .get(session_id)
            .is_some_and(|session| &session.connection_id == connection_id);
        if matches {
            sessions.remove(session_id);
        }
        matches
    }

    /// Returns a replayable batch after dropping only frames explicitly acked by the client.
    pub fn poll(&self, session_id: &str, after: u64) -> Result<ServerBatch, PollError> {
        let mut sessions = self.sessions.lock().expect("session registry poisoned");
        let Some(session) = sessions.get_mut(session_id) else {
            return Err(PollError::UnknownSession);
        };
        session.last_activity = Instant::now();
        if after >= session.next_server_sequence {
            return Err(PollError::InvalidAcknowledgement);
        }
        if session
            .server_frames
            .front()
            .is_some_and(|frame| after.saturating_add(1) < frame.sequence)
        {
            return Err(PollError::ReplayExpired);
        }
        while session
            .server_frames
            .front()
            .is_some_and(|frame| frame.sequence <= after)
        {
            session.server_frames.pop_front();
        }
        Ok(ServerBatch {
            frames: session.server_frames.iter().cloned().collect(),
        })
    }
}

impl ReliableSessionReaper {
    fn start(
        sessions: Weak<Mutex<HashMap<String, ReliableSession>>>,
        idle_timeout: Duration,
    ) -> Self {
        let (shutdown, receiver) = mpsc::channel();
        let stopped = Arc::new(AtomicBool::new(false));
        let worker_stopped = stopped.clone();
        let sweep_interval = idle_timeout
            .min(Duration::from_secs(1))
            .max(Duration::from_millis(1));
        let worker = std::thread::Builder::new()
            .name("openaide-reliable-session-reaper".to_owned())
            .spawn(move || {
                loop {
                    match receiver.recv_timeout(sweep_interval) {
                        Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                    let Some(sessions) = sessions.upgrade() else {
                        break;
                    };
                    let now = Instant::now();
                    sessions
                        .lock()
                        .expect("session registry poisoned")
                        .retain(|_, session| {
                            now.duration_since(session.last_activity) < idle_timeout
                        });
                }
                worker_stopped.store(true, Ordering::Release);
            })
            .expect("reliable session reaper thread must start");
        Self {
            shutdown,
            worker: Mutex::new(Some(worker)),
            stopped,
        }
    }
}

impl Drop for ReliableSessionReaper {
    fn drop(&mut self) {
        let _ = self.shutdown.send(());
        if let Some(worker) = self
            .worker
            .lock()
            .expect("session reaper lock poisoned")
            .take()
        {
            let _ = worker.join();
        }
        debug_assert!(self.stopped.load(Ordering::Acquire));
    }
}

#[cfg(test)]
#[path = "sessions_tests.rs"]
mod tests;
