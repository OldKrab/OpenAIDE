use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::client_lifecycle::ConnectionId;
use crate::logging;

use super::DeliverySignal;

// The transport-independent half of a resumable RPC session: sequencing,
// duplicate suppression, bounded replay, and link ownership. A link (held HTTP
// poll, WebSocket) only moves frames and maps these outcomes onto its wire.

pub(crate) const MAX_SERVER_REPLAY_FRAMES: usize = 1_024;

#[derive(Debug, Clone)]
pub(crate) struct ReliableSessionRegistry {
    server_id: String,
    sessions: Arc<Mutex<HashMap<String, ReliableSession>>>,
    /// Wakes the session's link when a frame is queued for the client.
    deliveries: DeliverySignal,
}

#[derive(Debug)]
struct ReliableSession {
    connection_id: ConnectionId,
    last_client_sequence: u64,
    next_server_sequence: u64,
    server_frames: VecDeque<ServerFrame>,
    /// Identifies the newest pushing link; an older one must stop writing.
    link_epoch: u64,
}

/// A pushing link's claim on one session. Attaching again supersedes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttachedLink {
    pub session_id: String,
    pub server_id: String,
    pub connection_id: ConnectionId,
    /// Highest client sequence already accepted; the client resends later frames.
    pub last_client_sequence: u64,
    epoch: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttachError {
    UnknownSession,
    WrongConnection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenedSession {
    pub session_id: String,
    pub server_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AcceptClientFrame {
    Accepted,
    Duplicate,
    Gap { expected: u64 },
    UnknownSession,
    WrongConnection,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServerFrame {
    pub sequence: u64,
    pub message: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServerBatch {
    pub frames: Vec<ServerFrame>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PollError {
    UnknownSession,
    InvalidAcknowledgement,
    ReplayExpired,
}

impl ReliableSessionRegistry {
    #[cfg(test)]
    pub fn new(server_id: impl Into<String>) -> Self {
        Self::with_delivery_signal(server_id, DeliverySignal::default())
    }

    pub fn with_delivery_signal(server_id: impl Into<String>, deliveries: DeliverySignal) -> Self {
        Self {
            server_id: server_id.into(),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            deliveries,
        }
    }

    /// Binds a pushing link to an existing session, or to a new one when the
    /// client has none to resume. The previous link, if any, loses ownership.
    pub fn attach_link(
        &self,
        connection_id: ConnectionId,
        resume_session_id: Option<&str>,
    ) -> Result<AttachedLink, AttachError> {
        let session_id = match resume_session_id {
            Some(session_id) => session_id.to_string(),
            None => self.open(connection_id.clone()).session_id,
        };
        let attached = {
            let mut sessions = self.sessions.lock().expect("session registry poisoned");
            let Some(session) = sessions.get_mut(&session_id) else {
                return Err(AttachError::UnknownSession);
            };
            if session.connection_id != connection_id {
                return Err(AttachError::WrongConnection);
            }
            session.link_epoch += 1;
            AttachedLink {
                session_id,
                server_id: self.server_id.clone(),
                connection_id,
                last_client_sequence: session.last_client_sequence,
                epoch: session.link_epoch,
            }
        };
        // The superseded link is parked on the signal; let it observe the loss.
        self.deliveries.notify();
        Ok(attached)
    }

    pub fn link_is_current(&self, link: &AttachedLink) -> bool {
        self.sessions
            .lock()
            .expect("session registry poisoned")
            .get(&link.session_id)
            .is_some_and(|session| session.link_epoch == link.epoch)
    }

    pub fn open(&self, connection_id: ConnectionId) -> OpenedSession {
        let session_id = Uuid::new_v4().to_string();
        let connection_id_value = connection_id.as_str().to_string();
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
                    link_epoch: 0,
                },
            );
        logging::info(
            "reliable_session_opened",
            serde_json::json!({
                "session_id": session_id.clone(),
                "connection_id": connection_id_value,
            }),
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
            logging::warn(
                "reliable_session_frame_rejected",
                serde_json::json!({
                    "session_id": session_id,
                    "connection_id": connection_id.as_str(),
                    "sequence": sequence,
                    "reason": "unknown_session",
                }),
            );
            return AcceptClientFrame::UnknownSession;
        };
        if &session.connection_id != connection_id {
            logging::warn(
                "reliable_session_frame_rejected",
                serde_json::json!({
                    "session_id": session_id,
                    "connection_id": connection_id.as_str(),
                    "sequence": sequence,
                    "reason": "wrong_connection",
                }),
            );
            return AcceptClientFrame::WrongConnection;
        }
        if sequence <= session.last_client_sequence {
            logging::info(
                "reliable_session_duplicate_frame",
                serde_json::json!({
                    "session_id": session_id,
                    "sequence": sequence,
                    "last_client_sequence": session.last_client_sequence,
                }),
            );
            return AcceptClientFrame::Duplicate;
        }
        let expected = session.last_client_sequence + 1;
        if sequence != expected {
            logging::warn(
                "reliable_session_frame_rejected",
                serde_json::json!({
                    "session_id": session_id,
                    "connection_id": connection_id.as_str(),
                    "sequence": sequence,
                    "expected_sequence": expected,
                    "reason": "sequence_gap",
                }),
            );
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
        drop(sessions);
        self.deliveries.notify();
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
        self.acknowledge(session_id, after)?;
        Ok(ServerBatch {
            frames: self.frames_after(session_id, after)?,
        })
    }

    /// Drops frames the client has fully applied. Frames merely written to a
    /// link stay replayable until this acknowledgement arrives.
    pub fn acknowledge(&self, session_id: &str, through: u64) -> Result<(), PollError> {
        let mut sessions = self.sessions.lock().expect("session registry poisoned");
        let session = checked_session(&mut sessions, session_id, through)?;
        while session
            .server_frames
            .front()
            .is_some_and(|frame| frame.sequence <= through)
        {
            session.server_frames.pop_front();
        }
        Ok(())
    }

    /// Retained frames later than `after`, without acknowledging anything.
    pub fn frames_after(
        &self,
        session_id: &str,
        after: u64,
    ) -> Result<Vec<ServerFrame>, PollError> {
        let mut sessions = self.sessions.lock().expect("session registry poisoned");
        let session = checked_session(&mut sessions, session_id, after)?;
        Ok(session
            .server_frames
            .iter()
            .filter(|frame| frame.sequence > after)
            .cloned()
            .collect())
    }
}

/// Validates a client cursor against the retained replay window.
fn checked_session<'a>(
    sessions: &'a mut HashMap<String, ReliableSession>,
    session_id: &str,
    after: u64,
) -> Result<&'a mut ReliableSession, PollError> {
    let Some(session) = sessions.get_mut(session_id) else {
        logging::warn(
            "reliable_session_poll_rejected",
            serde_json::json!({
                "session_id": session_id,
                "after_sequence": after,
                "reason": "unknown_session",
            }),
        );
        return Err(PollError::UnknownSession);
    };
    if after >= session.next_server_sequence {
        logging::warn(
            "reliable_session_poll_rejected",
            serde_json::json!({
                "session_id": session_id,
                "after_sequence": after,
                "next_server_sequence": session.next_server_sequence,
                "reason": "invalid_acknowledgement",
            }),
        );
        return Err(PollError::InvalidAcknowledgement);
    }
    if session
        .server_frames
        .front()
        .is_some_and(|frame| after.saturating_add(1) < frame.sequence)
    {
        logging::warn(
            "reliable_session_poll_rejected",
            serde_json::json!({
                "session_id": session_id,
                "after_sequence": after,
                "first_available_sequence": session.server_frames.front().map(|frame| frame.sequence),
                "reason": "replay_expired",
            }),
        );
        return Err(PollError::ReplayExpired);
    }
    Ok(session)
}

#[cfg(test)]
#[path = "reliable_session_tests.rs"]
mod tests;
