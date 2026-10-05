//! A pushing link's view of one reliable session. The link authenticates once,
//! then exchanges sequenced frames; wire framing stays with the listener.

use std::sync::atomic::{AtomicBool, Ordering};

use openaide_app_server_protocol::methods::{AGENT_AUTHENTICATE, CLIENT_HEARTBEAT};
use serde_json::{json, Value};

use crate::client_lifecycle::{AppServerTime, ConnectionId};
use crate::logging;
use crate::protocol_edge::reliable_session::{
    AcceptClientFrame, AttachError, AttachedLink, PollError, ServerFrame,
};
use crate::protocol_edge::DeliverySignal;

use super::protocol::{valid_connection_id, LocalHttpProtocolHandler};
use super::session_dispatch::{
    dispatch_accepted_session_frame, is_agent_authenticate_request, queue_connection_deliveries,
    AcceptedReliableUpload,
};
use super::{auth_status, AuthStatus};

/// Why a link has to stop. Only an interrupted socket may resume the session;
/// every reason here is the server telling the client something definite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LinkClose {
    Unauthorized,
    Forbidden,
    InvalidConnectionId,
    WrongConnection,
    /// The session, or the client it belongs to, no longer exists.
    SessionExpired,
    /// Frames the client still needs have left the replay window.
    ReplayExpired,
    InvalidAcknowledgement,
    SequenceGap,
    /// The session's client attached a newer link.
    Superseded,
    /// The frame's RPC envelope was rejected before dispatch.
    FrameRejected,
}

impl LinkClose {
    pub(crate) fn reason_code(self) -> &'static str {
        match self {
            Self::Unauthorized => "unauthorized",
            Self::Forbidden => "forbidden",
            Self::InvalidConnectionId => "invalid_connection_id",
            Self::WrongConnection => "wrong_connection",
            Self::SessionExpired => "session_expired",
            Self::ReplayExpired => "replay_expired",
            Self::InvalidAcknowledgement => "invalid_acknowledgement",
            Self::SequenceGap => "sequence_gap",
            Self::Superseded => "superseded",
            Self::FrameRejected => "frame_rejected",
        }
    }
}

impl From<PollError> for LinkClose {
    fn from(error: PollError) -> Self {
        match error {
            PollError::UnknownSession => Self::SessionExpired,
            PollError::InvalidAcknowledgement => Self::InvalidAcknowledgement,
            PollError::ReplayExpired => Self::ReplayExpired,
        }
    }
}

pub(crate) struct SessionLink {
    protocol: LocalHttpProtocolHandler,
    attached: AttachedLink,
    /// A session carries no client until its first frame (`client/initialize`)
    /// has run, so missing liveness only means expiry after that point.
    client_frame_dispatched: AtomicBool,
}

impl LocalHttpProtocolHandler {
    /// Authenticates a link and binds it to a new or resumed session.
    /// `received_through` is the client's cursor into the server sequence.
    pub(crate) fn open_session_link(
        &self,
        authorization: Option<&str>,
        connection_id: Option<&str>,
        resume_session_id: Option<&str>,
        received_through: u64,
    ) -> Result<SessionLink, LinkClose> {
        match auth_status(authorization, &self.auth_token) {
            AuthStatus::Authorized => {}
            AuthStatus::Missing => return Err(LinkClose::Unauthorized),
            AuthStatus::Invalid => return Err(LinkClose::Forbidden),
        }
        let connection_id =
            valid_connection_id(connection_id).ok_or(LinkClose::InvalidConnectionId)?;
        let attached = self
            .sessions
            .attach_link(connection_id, resume_session_id)
            .map_err(|error| match error {
                AttachError::UnknownSession => LinkClose::SessionExpired,
                AttachError::WrongConnection => LinkClose::WrongConnection,
            })?;
        self.sessions
            .acknowledge(&attached.session_id, received_through)?;
        Ok(SessionLink {
            client_frame_dispatched: AtomicBool::new(attached.last_client_sequence > 0),
            protocol: self.clone(),
            attached,
        })
    }
}

impl SessionLink {
    pub(crate) fn session_id(&self) -> &str {
        &self.attached.session_id
    }

    pub(crate) fn server_id(&self) -> &str {
        &self.attached.server_id
    }

    pub(crate) fn connection_id(&self) -> &ConnectionId {
        &self.attached.connection_id
    }

    /// Highest client sequence accepted before this link attached.
    pub(crate) fn client_received_through(&self) -> u64 {
        self.attached.last_client_sequence
    }

    pub(crate) fn delivery_signal(&self) -> DeliverySignal {
        self.protocol.gateway.delivery_signal()
    }

    /// Claims a client frame. `None` is a duplicate: acknowledge it, skip it.
    pub(crate) fn accept_frame(
        &self,
        sequence: u64,
        message: Value,
    ) -> Result<Option<AcceptedReliableUpload>, LinkClose> {
        let mut accepted_message = None;
        let outcome = self.protocol.sessions.accept_client_frame(
            &self.attached.session_id,
            &self.attached.connection_id,
            sequence,
            message,
            |message| accepted_message = Some(message),
        );
        match outcome {
            AcceptClientFrame::Accepted => {
                Ok(accepted_message.map(|message| AcceptedReliableUpload {
                    session_id: self.attached.session_id.clone(),
                    message,
                }))
            }
            AcceptClientFrame::Duplicate => Ok(None),
            AcceptClientFrame::Gap { .. } => Err(LinkClose::SequenceGap),
            AcceptClientFrame::UnknownSession => Err(LinkClose::SessionExpired),
            AcceptClientFrame::WrongConnection => Err(LinkClose::WrongConnection),
        }
    }

    /// Runs an accepted frame; its replies are queued on the session.
    pub(crate) fn dispatch_frame(&self, accepted: AcceptedReliableUpload) -> Result<(), LinkClose> {
        let method = accepted
            .message
            .get("method")
            .and_then(Value::as_str)
            .map(str::to_string);
        if !is_agent_authenticate_request(&accepted.message) {
            let outcome = self.dispatch_logged(accepted, method.as_deref());
            self.client_frame_dispatched.store(true, Ordering::Release);
            return outcome;
        }
        // Authentication can wait indefinitely for the user. Running it off the
        // link's ordered dispatch lets agent/cancelAuthenticate reach it.
        let deferred = Self {
            protocol: self.protocol.clone(),
            attached: self.attached.clone(),
            client_frame_dispatched: AtomicBool::new(true),
        };
        std::thread::spawn(move || {
            let _ = deferred.dispatch_logged(accepted, Some(AGENT_AUTHENTICATE));
        });
        Ok(())
    }

    fn dispatch_logged(
        &self,
        accepted: AcceptedReliableUpload,
        method: Option<&str>,
    ) -> Result<(), LinkClose> {
        let should_log = method != Some(CLIENT_HEARTBEAT);
        let started_at = std::time::Instant::now();
        if should_log {
            logging::info(
                "session_link_request_started",
                json!({
                    "connection_id": self.attached.connection_id.as_str(),
                    "session_id": self.attached.session_id,
                    "method": method,
                }),
            );
        }
        let now = AppServerTime::now();
        let gateway = self.protocol.gateway.clone();
        let response = dispatch_accepted_session_frame(
            self.attached.connection_id.clone(),
            accepted,
            &self.protocol.sessions,
            move |connection_id, message| gateway.handle_inbound(connection_id, message, now),
        );
        let outcome = if response.status == 204 {
            Ok(())
        } else {
            Err(LinkClose::FrameRejected)
        };
        if should_log {
            logging::info(
                "session_link_request_completed",
                json!({
                    "connection_id": self.attached.connection_id.as_str(),
                    "session_id": self.attached.session_id,
                    "method": method,
                    "outcome": if outcome.is_ok() { "completed" } else { "rejected" },
                    "duration_ms": started_at.elapsed().as_millis(),
                }),
            );
        }
        outcome
    }

    /// Drops frames the client reports as fully applied.
    pub(crate) fn acknowledge(&self, through: u64) -> Result<(), LinkClose> {
        Ok(self
            .protocol
            .sessions
            .acknowledge(&self.attached.session_id, through)?)
    }

    /// Renews client liveness, moves pending gateway deliveries onto the
    /// session, and returns the frames this link has not written yet.
    pub(crate) fn pump(&self, written_through: u64) -> Result<Vec<ServerFrame>, LinkClose> {
        let sessions = &self.protocol.sessions;
        if !sessions.link_is_current(&self.attached) {
            return Err(LinkClose::Superseded);
        }
        let gateway = &self.protocol.gateway;
        let connection_id = &self.attached.connection_id;
        let now = AppServerTime::now();
        if gateway.observe_connection_activity(connection_id, now) {
            queue_connection_deliveries(
                sessions,
                &self.attached.session_id,
                connection_id.clone(),
                gateway.drain_event_deliveries_for_connection(connection_id),
                gateway.drain_server_requests_for_connection(connection_id, now),
            );
        } else if self.client_frame_dispatched.load(Ordering::Acquire) {
            return Err(LinkClose::SessionExpired);
        }
        Ok(sessions.frames_after(&self.attached.session_id, written_through)?)
    }
}
