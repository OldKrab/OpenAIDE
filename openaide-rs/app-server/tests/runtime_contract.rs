use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use openaide_app_server::agent::events::{
    AgentEvent, AgentPermissionOption, AgentPermissionOptionKind, AgentPermissionRequest,
    AgentToolCall, AgentToolCallRef, AgentToolCallStatus,
};
use openaide_app_server::agent::mock::MockAgent;
use openaide_app_server::agent::{
    AgentEventSink, AgentLoadedSession, AgentMetadataField, AgentPrompt, AgentRuntime,
    AgentSession, AgentSessionDelete, AgentSessionEventSink, AgentSessionLoad,
    AgentSessionMetadataUpdate, AgentSessionResume, AgentSessionStart,
};
use openaide_app_server::protocol::errors::RuntimeError;
use openaide_app_server::protocol::host::HostBridge;
use openaide_app_server::protocol::model::{
    ActivityStatus, AgentCommand, AgentCommandsCatalog, Attachment, ConfigOption,
    ConfigOptionCategory, ConfigOptionValue, ConfigOptionsCatalog, ConfigOptionsStatus,
    InterruptionReason, IsolationKind, NormalizedMessage, PermissionDecision, TaskSnapshot,
    TaskStatus,
};
use openaide_app_server::protocol::params::{
    DeleteMode, PermissionRespondParams, SessionPromptParams, TaskCreateMode, TaskCreateParams,
    TaskDeleteParams, TaskIdParams, TaskListParams, TaskSnapshotParams,
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
