use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::client_lifecycle::ConnectionId;

pub(super) const MAX_SERVER_REPLAY_FRAMES: usize = 1_024;

#[derive(Debug, Clone)]
pub(super) struct ReliableSessionRegistry {
    server_id: String,
    sessions: Arc<Mutex<HashMap<String, ReliableSession>>>,
}

#[derive(Debug)]
struct ReliableSession {
    connection_id: ConnectionId,
    last_client_sequence: u64,
    next_server_sequence: u64,
    server_frames: VecDeque<ServerFrame>,
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
        Self {
            server_id: server_id.into(),
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn open(&self, connection_id: ConnectionId) -> OpenedSession {
        let session_id = Uuid::new_v4().to_string();
        self.sessions
            .lock()
            .expect("session registry poisoned")
            .insert(
                session_id.clone(),
                ReliableSession {
                    connection_id,
                    last_client_sequence: 0,
                    next_server_sequence: 1,
                    server_frames: VecDeque::new(),
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
        self.sessions
            .lock()
            .expect("session registry poisoned")
            .get(session_id)
            .map(|session| session.connection_id.clone())
    }

    /// Returns a replayable batch after dropping only frames explicitly acked by the client.
    pub fn poll(&self, session_id: &str, after: u64) -> Result<ServerBatch, PollError> {
        let mut sessions = self.sessions.lock().expect("session registry poisoned");
        let Some(session) = sessions.get_mut(session_id) else {
            return Err(PollError::UnknownSession);
        };
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

#[cfg(test)]
#[path = "sessions_tests.rs"]
mod tests;
