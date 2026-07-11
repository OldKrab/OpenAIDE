use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{ClientInstanceId, ProjectId, TaskId};
use crate::snapshot::ClientSnapshot;

pub const APP_SERVER_PROTOCOL_VERSION: &str = "1";

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
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub snapshot: ClientSnapshot,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ShellCapability {
    OpenExternal,
    RevealFile,
    ResolveFileReveal,
    PickLocalFile,
    OpenTerminal,
    ReadSecret,
    WriteSecret,
    ShowNotification,
}

#[cfg(test)]
mod tests;
