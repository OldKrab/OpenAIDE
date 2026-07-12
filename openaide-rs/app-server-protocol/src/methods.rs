use serde::{Deserialize, Serialize};

use crate::agent::{
    AgentAuthenticateParams, AgentAuthenticateResult, AgentCreateCustomParams,
    AgentCreateCustomResult, AgentDeleteCustomParams, AgentDeleteCustomResult,
    AgentListSessionsParams, AgentListSessionsResult, AgentProbeParams, AgentProbeResult,
    AgentReplaceCustomParams, AgentReplaceCustomResult, AgentSetEnabledParams,
    AgentSetEnabledResult, AgentSettingsDetailsParams, AgentSettingsDetailsResult,
    AgentUpdateCustomMetadataParams, AgentUpdateCustomMetadataResult,
};
use crate::attachment::{
    AttachmentConfirmEmbeddedParams, AttachmentConfirmEmbeddedResult,
    AttachmentCreateEmbeddedCandidateParams, AttachmentCreateEmbeddedCandidateResult,
    AttachmentCreateFileReferenceParams, AttachmentCreateFileReferenceResult,
    AttachmentCreatePastedImageParams, AttachmentCreatePastedImageResult,
    AttachmentListDirectoryParams, AttachmentListDirectoryResult, AttachmentListRootsParams,
    AttachmentListRootsResult, AttachmentRefreshHandlesParams, AttachmentRefreshHandlesResult,
    AttachmentReleaseHandlesParams, AttachmentReleaseHandlesResult, AttachmentRevealParams,
    AttachmentRevealResult,
};
use crate::client::{
    ClientCapabilitiesChangedParams, ClientCapabilitiesChangedResult, ClientHeartbeatParams,
    ClientHeartbeatResult, ClientProbeParams, ClientProbeResult, InitializeParams,
    InitializeResult,
};
use crate::diagnostics::{RuntimeDiagnosticsParams, RuntimeDiagnosticsResult};
use crate::envelopes::{ClientRequestEnvelope, RequestMeta, ResponseEnvelope, ResponseMeta};
use crate::server_requests::{ShellResolveFileRevealParams, ShellResolveFileRevealResult};
use crate::settings::{
    AppPreferencesParams, AppPreferencesResult, AppPreferencesUpdateParams, RuntimeSettingsParams,
    RuntimeSettingsResult, RuntimeSettingsUpdateParams, SettingsMcpServersParams,
    SettingsMcpServersResult, SettingsSkillsParams, SettingsSkillsResult,
};
use crate::state::{
    StateSubscribeParams, StateSubscribeResult, StateUnsubscribeParams, StateUnsubscribeResult,
};
use crate::support::{SupportRecoverStuckSessionsParams, SupportRecoverStuckSessionsResult};
use crate::task::{
    TaskAdoptNativeSessionParams, TaskAdoptNativeSessionResult, TaskCancelParams, TaskCancelResult,
    TaskChatPageParams, TaskChatPageResult, TaskCreateParams, TaskCreateResult, TaskDiscardParams,
    TaskDiscardResult, TaskListParams, TaskListResult, TaskMarkReadParams, TaskMarkReadResult,
    TaskOpenParams, TaskOpenResult, TaskRetryHistorySyncParams, TaskRetryHistorySyncResult,
    TaskSendParams, TaskSendResult, TaskSetArchivedParams, TaskSetArchivedResult,
    TaskSetConfigOptionParams, TaskSetConfigOptionResult, TaskToolDetailParams,
    TaskToolDetailResult,
};
use crate::workspace::{
    WorkspaceListDirectoryParams, WorkspaceListDirectoryResult, WorkspaceListRootsParams,
    WorkspaceListRootsResult,
};

mod names;

pub use names::*;

pub trait ProtocolMethod {
    const METHOD: &'static str;

    type Params: Serialize + for<'de> Deserialize<'de>;
    type Result: Serialize + for<'de> Deserialize<'de>;

    fn request(params: Self::Params, meta: RequestMeta) -> ClientRequestEnvelope<Self::Params>
    where
        Self: Sized,
    {
        ClientRequestEnvelope::new(Self::METHOD, params, meta)
    }

    fn response(result: Self::Result, meta: ResponseMeta) -> ResponseEnvelope<Self::Result>
    where
        Self: Sized,
    {
        ResponseEnvelope::new(result, meta)
    }
}

macro_rules! protocol_method {
    ($name:ident, $method:expr, $params:ty, $result:ty) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct $name;

        impl ProtocolMethod for $name {
            const METHOD: &'static str = $method;

            type Params = $params;
            type Result = $result;
        }
    };
}

protocol_method!(
    ClientProbe,
    CLIENT_PROBE,
    ClientProbeParams,
    ClientProbeResult
);
protocol_method!(
    ClientInitialize,
    CLIENT_INITIALIZE,
    InitializeParams,
    InitializeResult
);
protocol_method!(
    ClientCapabilitiesChanged,
    CLIENT_CAPABILITIES_CHANGED,
    ClientCapabilitiesChangedParams,
    ClientCapabilitiesChangedResult
);
protocol_method!(
    ClientHeartbeat,
    CLIENT_HEARTBEAT,
    ClientHeartbeatParams,
    ClientHeartbeatResult
);
protocol_method!(
    StateSubscribe,
    STATE_SUBSCRIBE,
    StateSubscribeParams,
    StateSubscribeResult
);
protocol_method!(
    StateUnsubscribe,
    STATE_UNSUBSCRIBE,
    StateUnsubscribeParams,
    StateUnsubscribeResult
);
protocol_method!(
    DiagnosticsGetRuntime,
    DIAGNOSTICS_GET_RUNTIME,
    RuntimeDiagnosticsParams,
    RuntimeDiagnosticsResult
);
protocol_method!(
    SupportRecoverStuckSessions,
    SUPPORT_RECOVER_STUCK_SESSIONS,
    SupportRecoverStuckSessionsParams,
    SupportRecoverStuckSessionsResult
);
protocol_method!(AgentProbe, AGENT_PROBE, AgentProbeParams, AgentProbeResult);
protocol_method!(
    AgentAuthenticate,
    AGENT_AUTHENTICATE,
    AgentAuthenticateParams,
    AgentAuthenticateResult
);
protocol_method!(
    AgentListSessions,
    AGENT_LIST_SESSIONS,
    AgentListSessionsParams,
    AgentListSessionsResult
);
protocol_method!(
    AgentCreateCustom,
    AGENT_CREATE_CUSTOM,
    AgentCreateCustomParams,
    AgentCreateCustomResult
);
protocol_method!(
    AgentUpdateCustomMetadata,
    AGENT_UPDATE_CUSTOM_METADATA,
    AgentUpdateCustomMetadataParams,
    AgentUpdateCustomMetadataResult
);
protocol_method!(
    AgentReplaceCustom,
    AGENT_REPLACE_CUSTOM,
    AgentReplaceCustomParams,
    AgentReplaceCustomResult
);
protocol_method!(
    AgentDeleteCustom,
    AGENT_DELETE_CUSTOM,
    AgentDeleteCustomParams,
    AgentDeleteCustomResult
);
protocol_method!(
    AgentSetEnabled,
    AGENT_SET_ENABLED,
    AgentSetEnabledParams,
    AgentSetEnabledResult
);
protocol_method!(
    SettingsGetAgentDetails,
    SETTINGS_GET_AGENT_DETAILS,
    AgentSettingsDetailsParams,
    AgentSettingsDetailsResult
);
protocol_method!(
    SettingsGetMcpServers,
    SETTINGS_GET_MCP_SERVERS,
    SettingsMcpServersParams,
    SettingsMcpServersResult
);
protocol_method!(
    SettingsGetSkills,
    SETTINGS_GET_SKILLS,
    SettingsSkillsParams,
    SettingsSkillsResult
);
protocol_method!(
    SettingsGetPreferences,
    SETTINGS_GET_PREFERENCES,
    AppPreferencesParams,
    AppPreferencesResult
);
protocol_method!(
    SettingsUpdatePreferences,
    SETTINGS_UPDATE_PREFERENCES,
    AppPreferencesUpdateParams,
    AppPreferencesResult
);
protocol_method!(
    SettingsGetRuntime,
    SETTINGS_GET_RUNTIME,
    RuntimeSettingsParams,
    RuntimeSettingsResult
);
protocol_method!(
    SettingsUpdateRuntime,
    SETTINGS_UPDATE_RUNTIME,
    RuntimeSettingsUpdateParams,
    RuntimeSettingsResult
);
protocol_method!(
    WorkspaceListRoots,
    WORKSPACE_LIST_ROOTS,
    WorkspaceListRootsParams,
    WorkspaceListRootsResult
);
protocol_method!(
    WorkspaceListDirectory,
    WORKSPACE_LIST_DIRECTORY,
    WorkspaceListDirectoryParams,
    WorkspaceListDirectoryResult
);
protocol_method!(
    AttachmentListRoots,
    ATTACHMENT_LIST_ROOTS,
    AttachmentListRootsParams,
    AttachmentListRootsResult
);
protocol_method!(
    AttachmentListDirectory,
    ATTACHMENT_LIST_DIRECTORY,
    AttachmentListDirectoryParams,
    AttachmentListDirectoryResult
);
protocol_method!(
    AttachmentCreateFileReference,
    ATTACHMENT_CREATE_FILE_REFERENCE,
    AttachmentCreateFileReferenceParams,
    AttachmentCreateFileReferenceResult
);
protocol_method!(
    AttachmentCreatePastedImage,
    ATTACHMENT_CREATE_PASTED_IMAGE,
    AttachmentCreatePastedImageParams,
    AttachmentCreatePastedImageResult
);
protocol_method!(
    AttachmentCreateEmbeddedCandidate,
    ATTACHMENT_CREATE_EMBEDDED_CANDIDATE,
    AttachmentCreateEmbeddedCandidateParams,
    AttachmentCreateEmbeddedCandidateResult
);
protocol_method!(
    AttachmentConfirmEmbedded,
    ATTACHMENT_CONFIRM_EMBEDDED,
    AttachmentConfirmEmbeddedParams,
    AttachmentConfirmEmbeddedResult
);
protocol_method!(
    AttachmentRefreshHandles,
    ATTACHMENT_REFRESH_HANDLES,
    AttachmentRefreshHandlesParams,
    AttachmentRefreshHandlesResult
);
protocol_method!(
    AttachmentReleaseHandles,
    ATTACHMENT_RELEASE_HANDLES,
    AttachmentReleaseHandlesParams,
    AttachmentReleaseHandlesResult
);
protocol_method!(
    AttachmentReveal,
    ATTACHMENT_REVEAL,
    AttachmentRevealParams,
    AttachmentRevealResult
);
protocol_method!(
    ShellResolveFileReveal,
    SHELL_RESOLVE_FILE_REVEAL,
    ShellResolveFileRevealParams,
    ShellResolveFileRevealResult
);
protocol_method!(TaskCreate, TASK_CREATE, TaskCreateParams, TaskCreateResult);
protocol_method!(
    TaskAdoptNativeSession,
    TASK_ADOPT_NATIVE_SESSION,
    TaskAdoptNativeSessionParams,
    TaskAdoptNativeSessionResult
);
protocol_method!(TaskSend, TASK_SEND, TaskSendParams, TaskSendResult);
protocol_method!(
    TaskSetConfigOption,
    TASK_SET_CONFIG_OPTION,
    TaskSetConfigOptionParams,
    TaskSetConfigOptionResult
);
protocol_method!(TaskCancel, TASK_CANCEL, TaskCancelParams, TaskCancelResult);
protocol_method!(TaskOpen, TASK_OPEN, TaskOpenParams, TaskOpenResult);
protocol_method!(
    TaskRetryHistorySync,
    TASK_RETRY_HISTORY_SYNC,
    TaskRetryHistorySyncParams,
    TaskRetryHistorySyncResult
);
protocol_method!(
    TaskMarkRead,
    TASK_MARK_READ,
    TaskMarkReadParams,
    TaskMarkReadResult
);
protocol_method!(
    TaskChatPage,
    TASK_CHAT_PAGE,
    TaskChatPageParams,
    TaskChatPageResult
);
protocol_method!(
    TaskToolDetail,
    TASK_TOOL_DETAIL,
    TaskToolDetailParams,
    TaskToolDetailResult
);
protocol_method!(TaskList, TASK_LIST, TaskListParams, TaskListResult);
protocol_method!(
    TaskDiscard,
    TASK_DISCARD,
    TaskDiscardParams,
    TaskDiscardResult
);
protocol_method!(
    TaskSetArchived,
    TASK_SET_ARCHIVED,
    TaskSetArchivedParams,
    TaskSetArchivedResult
);

#[cfg(test)]
mod tests;
