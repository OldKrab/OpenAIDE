use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{ClientInstanceId, ProjectId, TaskId};
use crate::snapshot::{ClientSnapshot, ProjectCollectionSnapshot, ProtocolVersion};

/// The App Server Protocol version, separate from the product version (ADR-0062).
///
/// Bump `minor` when adding methods, fields, or events. Bump `major` and reset
/// `minor` for any change an older client cannot ignore.
pub const APP_SERVER_PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion { major: 2, minor: 4 };

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ClientProbeParams {}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ClientProbeResult {
    pub state_root_fingerprint: String,
    pub protocol_version: String,
    pub app_version: String,
    pub lifecycle: ClientProbeLifecycle,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ClientHeartbeatParams {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ClientHeartbeatResult {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ClientDetachParams {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ClientDetachResult {}

/// Begins the App Server-owned quiescence boundary required before a Desktop update.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UpdateShutdownPrepareParams {
    pub attempt_id: String,
    pub stop_active_work: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum UpdateShutdownPrepareResult {
    Ready,
    Blocked { reason: UpdateShutdownBlockedReason },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum UpdateShutdownBlockedReason {
    ActiveWork,
    OtherClients,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UpdateShutdownCommitParams {
    pub attempt_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UpdateShutdownCommitResult {}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UpdateShutdownAbortParams {
    pub attempt_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UpdateShutdownAbortResult {}

/// Asks what replacing this App Server with another version would affect now. A client
/// that installed a different version shows the answer before the user confirms the
/// replacement (ADR-0069).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ClientReplacementImpactParams {}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ClientReplacementImpactResult {
    /// Initialized clients other than the caller, including those inside their reconnect
    /// grace, which a replacement would also disconnect.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub other_clients: Vec<AttachedClientSummary>,
    pub active_turns: u32,
    pub pending_task_requests: u32,
    /// Terminals whose shells a replacement would end (ADR-0071). Absent from App Servers
    /// before protocol 2.3.
    #[serde(default)]
    pub open_terminals: u32,
}

/// What a client reported about itself in `client/initialize`. Labels for the user, not
/// evidence.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttachedClientSummary {
    pub kind: ShellKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The product version the client reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_version: Option<String>,
    /// Omitted by clients installed together with their App Server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<ProtocolVersion>,
    /// The client stopped responding and is inside its reconnect grace.
    pub reconnecting: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ClientProbeLifecycle {
    Running,
    Draining,
    Stopping,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    pub client_instance_id: ClientInstanceId,
    pub shell: ShellDescriptor,
    pub requested_surface: RequestedSurface,
    #[serde(default, skip_serializing_if = "ClientCapabilities::is_empty")]
    pub capabilities: ClientCapabilities,
    /// Local workspace facts supplied by the App Shell for Project canonicalization.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workspace_roots: Vec<ClientWorkspaceRoot>,
    /// The protocol version the client was built against. A client that ships
    /// separately from its App Server must send it; an omitted version is not checked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<ProtocolVersion>,
    /// What the client is to the App Server; omitted for an ordinary client.
    #[serde(default, skip_serializing_if = "ClientRole::is_interactive")]
    pub role: ClientRole,
}

/// The part a client plays in the App Server's lifetime.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ClientRole {
    /// A client someone works in, such as an App Shell window or a Remote Device.
    #[default]
    Interactive,
    /// An App Shell's own windowless client that keeps the App Server alive for
    /// as long as the shell runs, because a hidden window's timers may stall.
    /// It counts toward the App Server's lifetime like any client, but it is
    /// not another owner when that shell's window prepares an update shutdown:
    /// the shell ends it itself once the window has detached.
    ShellHold,
}

impl ClientRole {
    pub fn is_interactive(&self) -> bool {
        matches!(self, Self::Interactive)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub snapshot: ClientSnapshot,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ClientCapabilitiesChangedParams {
    /// Omitted capabilities preserve the client's existing capability declaration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<ClientCapabilities>,
    /// A present list replaces this client's complete shell-reported workspace set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_roots: Option<Vec<ClientWorkspaceRoot>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ClientCapabilitiesChangedResult {
    pub projects: ProjectCollectionSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ClientWorkspaceRoot {
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ShellDescriptor {
    pub kind: ShellKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ShellKind {
    Web,
    Desktop,
    Mobile,
    VscodeExtension,
    /// The App Server's own `remote` command: a same-machine client with no
    /// window that keeps the App Server running for Remote Devices.
    Terminal,
    /// The App Server's own keeper on an SSH Computer: a same-machine client
    /// with no window that holds the App Server while it has work or a recent
    /// App Shell, then leaves (ADR-0069).
    Keeper,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RequestedSurface {
    Home,
    Project {
        project_id: ProjectId,
    },
    NewTask {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<ProjectId>,
    },
    Task {
        task_id: TaskId,
    },
    Settings {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        section: Option<SettingsSection>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum SettingsSection {
    Agents,
    McpServers,
    Skills,
    CommonSettings,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ClientCapabilities {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protocol: Vec<ClientProtocolCapability>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shell: Vec<ShellCapability>,
}

impl ClientCapabilities {
    pub fn is_empty(&self) -> bool {
        self.protocol.is_empty() && self.shell.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ClientProtocolCapability {
    Resync,
    RequestResponses,
    StableClientRequestIds,
    PermissionResponses,
    QuestionResponses,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ShellCapability {
    OpenExternal,
    RevealFile,
    ResolveFileReveal,
    PickLocalFile,
    /// Read nowhere and no longer advertised; Terminals are an App Server feature (ADR-0071)
    /// and VS Code opens its own through an App Shell capability. Clients up to protocol 2.2
    /// still send it, so it must keep deserializing.
    // TODO(protocol-3): remove with the next major protocol version, which drops 2.x clients.
    OpenTerminal,
    ReadSecret,
    WriteSecret,
    ShowNotification,
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
