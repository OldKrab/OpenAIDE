use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use openaide_app_server::agent::events::{AgentEvent, AgentToolCall, AgentToolCallStatus};
use openaide_app_server::agent::mock::MockAgent;
use openaide_app_server::agent::{
    AgentEventSink, AgentLoadedSession, AgentMetadataField, AgentPrompt, AgentRuntime,
    AgentSession, AgentSessionDelete, AgentSessionEventSink, AgentSessionKey, AgentSessionLoad,
    AgentSessionMetadataUpdate, AgentSessionResume, AgentSessionStart,
};
use openaide_app_server::protocol::errors::RuntimeError;
use openaide_app_server::protocol::host::HostBridge;
use openaide_app_server::protocol::model::{
    ActivityStatus, AgentCommand, AgentCommandsCatalog, AgentMessagePart, AgentMessageRole,
    Attachment, ConfigOption, ConfigOptionCategory, ConfigOptionValue, ConfigOptionsCatalog,
    ConfigOptionsStatus, InterruptionReason, IsolationKind, NormalizedMessage, TaskSnapshot,
    TaskStatus,
};
use openaide_app_server::protocol::params::{
    DeleteMode, SessionPromptParams, TaskCreateMode, TaskCreateParams, TaskDeleteParams,
    TaskIdParams, TaskSnapshotParams,
};
use openaide_app_server::storage::records::{TaskPreparationRecord, TaskRecord};
use openaide_app_server::storage::Store;
use openaide_app_server::task_events::TaskUpdateNotifier;
use openaide_app_server::tasks::TaskService;
use openaide_app_server::transport::shell_control::ShellControlDispatcher;
use openaide_app_server::Runtime;
use serde_json::{json, Value};
use tempfile::TempDir;

include!("runtime_contract/agent_setup.rs");
include!("runtime_contract/activity_stream.rs");
include!("runtime_contract/protocol_edge_stdio.rs");
include!("runtime_contract/shutdown_and_failures.rs");
include!("runtime_contract/task_creation.rs");
include!("runtime_contract/task_runtime.rs");
include!("runtime_contract/support.rs");

fn agent_text_event(text: impl Into<String>) -> AgentEvent {
    AgentEvent::MessageChunk {
        role: AgentMessageRole::Agent,
        part: AgentMessagePart::Text { text: text.into() },
        source_message_id: None,
    }
}

fn sourced_agent_text_event(text: impl Into<String>, source_message_id: &str) -> AgentEvent {
    AgentEvent::MessageChunk {
        role: AgentMessageRole::Agent,
        part: AgentMessagePart::Text { text: text.into() },
        source_message_id: Some(source_message_id.to_string()),
    }
}

fn normalized_agent_text(
    id: impl Into<String>,
    text: impl Into<String>,
    created_at: impl Into<String>,
) -> NormalizedMessage {
    NormalizedMessage::AgentMessage {
        id: id.into(),
        role: AgentMessageRole::Agent,
        parts: vec![AgentMessagePart::Text { text: text.into() }],
        created_at: created_at.into(),
    }
}

fn agent_message_text(message: &NormalizedMessage) -> Option<&str> {
    match message {
        NormalizedMessage::AgentMessage {
            role: AgentMessageRole::Agent,
            parts,
            ..
        } => match parts.as_slice() {
            [AgentMessagePart::Text { text }] => Some(text),
            _ => None,
        },
        _ => None,
    }
}
