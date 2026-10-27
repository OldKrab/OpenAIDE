//! Requests the attachment handle sends to its private event loop.

use std::sync::{mpsc, Arc};
use std::time::Instant;

use crate::agent::{
    AgentEventSink, AgentLoadedSession, AgentPrompt, AgentPromptOutcome, AgentSessionEventSink,
    AgentSessionLoad,
};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{ConfigOptionCurrentValue, ConfigOptionsCatalog};

use super::PromptRequestGuard;

pub(in crate::agent) enum AcpSessionCommand {
    SetEventSink {
        sink: Arc<dyn AgentSessionEventSink>,
    },
    Load {
        request: AgentSessionLoad,
        reply_tx: mpsc::Sender<Result<AgentLoadedSession, RuntimeError>>,
    },
    Prompt {
        prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
        done_tx: mpsc::Sender<Result<AgentPromptOutcome, RuntimeError>>,
        request_guard: PromptRequestGuard,
    },
    Steer {
        prompt: AgentPrompt,
        request_guard: PromptRequestGuard,
    },
    Delete {
        operation_id: String,
        reply_tx: mpsc::Sender<Result<(), RuntimeError>>,
    },
    /// Stops one Agent-reported background command of the active prompt.
    StopBackgroundCommand {
        command_id: String,
        reply_tx: mpsc::Sender<Result<(), RuntimeError>>,
    },
    /// Stops one Native Subagent, whether or not a prompt is active: a
    /// Subagent can outlive the prompt that started it.
    StopSubagent {
        native_session_id: String,
        reply_tx: mpsc::Sender<Result<(), RuntimeError>>,
    },
}

pub(in crate::agent) enum AcpSessionConfigCommand {
    SetConfigOption {
        agent_id: String,
        session_id: String,
        config_id: String,
        value: ConfigOptionCurrentValue,
        operation_id: String,
        queued_at: Instant,
        reply_tx: mpsc::Sender<Result<ConfigOptionsCatalog, RuntimeError>>,
    },
}
