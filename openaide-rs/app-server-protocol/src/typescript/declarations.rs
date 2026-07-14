use ts_rs::{Config, Dummy, TS};

use crate::agent::{
    AgentAuthenticateParams, AgentAuthenticateResult, AgentAuthenticateStatus,
    AgentCreateCustomParams, AgentCreateCustomResult, AgentDeleteCustomParams,
    AgentDeleteCustomResult, AgentListSessionsParams, AgentListSessionsResult, AgentListedSession,
    AgentProbeParams, AgentProbeResult, AgentReplaceCustomCleanup, AgentReplaceCustomConfirmation,
    AgentReplaceCustomHistoryPolicy, AgentReplaceCustomParams, AgentReplaceCustomResult,
    AgentSetEnabledParams, AgentSetEnabledResult, AgentSettingsAuthMethod, AgentSettingsDetail,
    AgentSettingsDetailsParams, AgentSettingsDetailsResult, AgentSettingsEnvRow,
    AgentSettingsSourceKind, AgentSettingsStatus, AgentSettingsTransport,
    AgentUpdateCustomMetadataParams, AgentUpdateCustomMetadataResult,
};
use crate::attachment::{
    AttachmentCandidateError, AttachmentCandidateErrorCode, AttachmentConfirmEmbeddedParams,
    AttachmentConfirmEmbeddedResult, AttachmentCreateEmbeddedCandidateParams,
    AttachmentCreateEmbeddedCandidateResult, AttachmentCreateFileReferenceParams,
    AttachmentCreateFileReferenceResult, AttachmentCreatePastedImageParams,
    AttachmentCreatePastedImageResult, AttachmentListDirectoryParams,
    AttachmentListDirectoryResult, AttachmentListRootsParams, AttachmentListRootsResult,
    AttachmentRefreshHandlesParams, AttachmentRefreshHandlesResult, AttachmentReleaseOutcome,
    AttachmentReleaseParams, AttachmentReleaseResult, AttachmentReleaseStatus,
    AttachmentResourceId, AttachmentRevealParams, AttachmentRevealResult,
    EmbeddedAttachmentCandidate, FileBrowserDirectory, FileBrowserEntry, FileBrowserEntryKind,
    FileBrowserRoot, PreSendAttachment,
};
use crate::client::{
    ClientCapabilities, ClientCapabilitiesChangedParams, ClientCapabilitiesChangedResult,
    ClientHeartbeatParams, ClientHeartbeatResult, ClientProbeLifecycle, ClientProbeParams,
    ClientProbeResult, ClientProtocolCapability, ClientWorkspaceRoot, InitializeParams,
    InitializeResult, RequestedSurface, SettingsSection, ShellCapability, ShellDescriptor,
    ShellKind,
};
use crate::diagnostics::{
    ActiveTaskDiagnosticsResult, DiagnosticsRedaction, RuntimeDiagnosticsParams,
    RuntimeDiagnosticsResult, RuntimeDiagnosticsStatus, TaskDiagnosticsResult,
};
use crate::envelopes::{
    ClientRequestEnvelope, ErrorEnvelope, RequestMeta, ResponseEnvelope, ResponseMeta,
    ServerRequestEnvelope,
};
use crate::errors::{ErrorTarget, ProtocolError, ProtocolErrorCode};
use crate::events::{
    AppServerEvent, AppServerEventPayload, EventScope, TaskChanges, TaskChatChange,
    TaskNavigationChange,
};
use crate::ids::{
    AgentConfigOptionId, AgentId, AttachmentCandidateId, AttachmentHandleId, AttachmentId,
    ClientInstanceId, ClientMutationId, ClientRequestId, EventCursor, FileBrowserEntryId,
    FileBrowserRootId, MessageId, ProjectId, RequestId, ServerId, StateRootId, TaskId,
    TaskListCursor, TurnId,
};
use crate::server_requests::{
    PendingRequestResolution, PendingRequestResolveParams, PendingRequestResolveResult,
    PermissionRequestOption, PermissionRequestOptionKind, PermissionRequestParams,
    PermissionRequestResponse, PermissionToolCallRef, QuestionField, QuestionOption,
    QuestionRequestParams, QuestionRequestResponse, QuestionStringFormat, QuestionValue,
    SecretReadParams, SecretReadResponse, ShellNotificationAction, ShellNotificationLevel,
    ShellResolveFileRevealParams, ShellResolveFileRevealResult, ShellRevealFileParams,
    ShellRevealFileResponse, ShellShowNotificationParams, ShellShowNotificationResponse,
};
use crate::settings::{
    AppPreferences, AppPreferencesParams, AppPreferencesPatch, AppPreferencesResult,
    AppPreferencesUpdateParams, ComposerSubmitShortcut, RuntimeAcpTraceSettings,
    RuntimeAcpTraceSettingsPatch, RuntimeDeveloperSettings, RuntimeDeveloperSettingsPatch,
    RuntimeSettingsParams, RuntimeSettingsResult, RuntimeSettingsUpdateParams,
    SettingsMcpServerRecord, SettingsMcpServerStatus, SettingsMcpServerTransport,
    SettingsMcpServersParams, SettingsMcpServersResult, SettingsProjectionAvailability,
    SettingsProjectionNotice, SettingsProjectionNoticeSeverity, SettingsScope, SettingsSkillRecord,
    SettingsSkillStatus, SettingsSkillsParams, SettingsSkillsResult,
};
use crate::snapshot::{
    ActivityStatus, ActivityStepSnapshot, AgentCapabilities, AgentCollectionSnapshot,
    AgentConfigOptionKind, AgentConfigOptionSnapshot, AgentConfigOptionValueSnapshot,
    AgentSlashCommandInputSnapshot, AgentSlashCommandSnapshot, AgentStatus, AgentSummary,
    AttachmentKind, AttachmentSnapshot, ChatItem, ChatItemStatus, ChatRole, ChatSnapshot,
    ClientSnapshot, ClientSnapshotScope, LiveSessionDataState, MessagePart,
    NewTaskDefaultsSnapshot, PendingAgentConfigChange, PendingRequestKind, PendingRequestScope,
    PendingRequestSnapshot, ProjectCollectionSnapshot, ProjectSummary, ProtocolVersion,
    QuestionMessageAction, QuestionMessageState, RecoveryAction, RecoverySnapshot,
    ServerCapabilities, ServerSnapshot, SettingsSnapshot, StateRootSnapshot,
    TaskAgentCommandsSnapshot, TaskAgentConfigSnapshot, TaskHistorySyncSnapshot, TaskLifecycle,
    TaskNavigationSnapshot, TaskPreparationAction, TaskPreparationSnapshot, TaskPreparationStep,
    TaskPreparationStepKind, TaskPreparationStepStatus, TaskSendBlocker, TaskSendBlockerKind,
    TaskSendCapabilitySnapshot, TaskSendCapabilityState, TaskSetupBlocker, TaskSetupBlockerKind,
    TaskSnapshot, TaskStatus, TaskSummary, TaskTitle, TaskTitleSource,
    ToolPermissionDecisionSnapshot, ToolPermissionOutcomeSnapshot,
};
use crate::state::{
    StateSubscribeParams, StateSubscribeResult, StateUnsubscribeParams, StateUnsubscribeResult,
    SubscriptionScope, SubscriptionSnapshot,
};
use crate::support::{SupportRecoverStuckSessionsParams, SupportRecoverStuckSessionsResult};
use crate::task::{
    ActivityToolContent, ActivityToolField, ActivityToolInput, ActivityToolLocation,
    ActivityToolOutput, ActivityToolValue, ComposerMessage, TaskAdoptNativeSessionParams,
    TaskAdoptNativeSessionResult, TaskCancelParams, TaskCancelResult, TaskChatPageParams,
    TaskChatPageResult, TaskCreateParams, TaskCreateResult, TaskDiscardParams, TaskDiscardResult,
    TaskListParams, TaskListResult, TaskMarkReadParams, TaskMarkReadResult, TaskOpenParams,
    TaskOpenResult, TaskSendParams, TaskSendResult, TaskSetArchivedParams, TaskSetArchivedResult,
    TaskSetConfigOptionParams, TaskSetConfigOptionResult, ToolDetailSnapshot,
};
use crate::workspace::{
    WorkspaceBrowserDirectory, WorkspaceBrowserEntry, WorkspaceBrowserRoot,
    WorkspaceListDirectoryParams, WorkspaceListDirectoryResult, WorkspaceListRootsParams,
    WorkspaceListRootsResult,
};

pub(super) fn push_protocol_declarations(output: &mut String, config: &Config) {
    push_decl::<AgentConfigOptionId>(output, config);
    push_decl::<AgentId>(output, config);
    push_decl::<AttachmentCandidateId>(output, config);
    push_decl::<AttachmentHandleId>(output, config);
    push_decl::<AttachmentId>(output, config);
    push_decl::<ClientInstanceId>(output, config);
    push_decl::<ClientMutationId>(output, config);
    push_decl::<ClientRequestId>(output, config);
    push_decl::<EventCursor>(output, config);
    push_decl::<FileBrowserEntryId>(output, config);
    push_decl::<FileBrowserRootId>(output, config);
    push_decl::<MessageId>(output, config);
    push_decl::<ProjectId>(output, config);
    push_decl::<RequestId>(output, config);
    push_decl::<ServerId>(output, config);
    push_decl::<StateRootId>(output, config);
    push_decl::<TaskId>(output, config);
    push_decl::<TaskListCursor>(output, config);
    push_decl::<TurnId>(output, config);

    push_decl::<ClientRequestEnvelope<Dummy>>(output, config);
    push_decl::<ResponseEnvelope<Dummy>>(output, config);
    push_decl::<ErrorEnvelope>(output, config);
    push_decl::<ServerRequestEnvelope<Dummy>>(output, config);
    push_decl::<RequestMeta>(output, config);
    push_decl::<ResponseMeta>(output, config);
    push_decl::<ProtocolError>(output, config);
    push_decl::<ProtocolErrorCode>(output, config);
    push_decl::<ErrorTarget>(output, config);

    push_decl::<ClientProbeParams>(output, config);
    push_decl::<ClientProbeResult>(output, config);
    push_decl::<ClientHeartbeatParams>(output, config);
    push_decl::<ClientHeartbeatResult>(output, config);
    push_decl::<ClientProbeLifecycle>(output, config);
    push_decl::<InitializeParams>(output, config);
    push_decl::<InitializeResult>(output, config);
    push_decl::<ClientCapabilitiesChangedParams>(output, config);
    push_decl::<ClientCapabilitiesChangedResult>(output, config);
    push_decl::<ClientWorkspaceRoot>(output, config);
    push_decl::<ShellDescriptor>(output, config);
    push_decl::<ShellKind>(output, config);
    push_decl::<RequestedSurface>(output, config);
    push_decl::<SettingsSection>(output, config);
    push_decl::<ClientCapabilities>(output, config);
    push_decl::<ClientProtocolCapability>(output, config);
    push_decl::<ShellCapability>(output, config);

    push_decl::<StateSubscribeParams>(output, config);
    push_decl::<StateSubscribeResult>(output, config);
    push_decl::<StateUnsubscribeParams>(output, config);
    push_decl::<StateUnsubscribeResult>(output, config);
    push_decl::<SubscriptionScope>(output, config);
    push_decl::<SubscriptionSnapshot>(output, config);

    push_decl::<RuntimeDiagnosticsParams>(output, config);
    push_decl::<RuntimeDiagnosticsResult>(output, config);
    push_decl::<RuntimeDiagnosticsStatus>(output, config);
    push_decl::<TaskDiagnosticsResult>(output, config);
    push_decl::<ActiveTaskDiagnosticsResult>(output, config);
    push_decl::<DiagnosticsRedaction>(output, config);

    push_decl::<AgentProbeParams>(output, config);
    push_decl::<AgentProbeResult>(output, config);
    push_decl::<AgentAuthenticateParams>(output, config);
    push_decl::<AgentAuthenticateResult>(output, config);
    push_decl::<AgentAuthenticateStatus>(output, config);
    push_decl::<AgentListSessionsParams>(output, config);
    push_decl::<AgentListSessionsResult>(output, config);
    push_decl::<AgentListedSession>(output, config);
    push_decl::<AgentCreateCustomParams>(output, config);
    push_decl::<AgentCreateCustomResult>(output, config);
    push_decl::<AgentUpdateCustomMetadataParams>(output, config);
    push_decl::<AgentUpdateCustomMetadataResult>(output, config);
    push_decl::<AgentReplaceCustomParams>(output, config);
    push_decl::<AgentReplaceCustomConfirmation>(output, config);
    push_decl::<AgentReplaceCustomCleanup>(output, config);
    push_decl::<AgentReplaceCustomHistoryPolicy>(output, config);
    push_decl::<AgentReplaceCustomResult>(output, config);
    push_decl::<AgentDeleteCustomParams>(output, config);
    push_decl::<AgentDeleteCustomResult>(output, config);
    push_decl::<AgentSetEnabledParams>(output, config);
    push_decl::<AgentSetEnabledResult>(output, config);
    push_decl::<AgentSettingsDetailsParams>(output, config);
    push_decl::<AgentSettingsDetailsResult>(output, config);
    push_decl::<AgentSettingsDetail>(output, config);
    push_decl::<AgentSettingsSourceKind>(output, config);
    push_decl::<AgentSettingsTransport>(output, config);
    push_decl::<AgentSettingsStatus>(output, config);
    push_decl::<AgentSettingsEnvRow>(output, config);
    push_decl::<AgentSettingsAuthMethod>(output, config);
    push_decl::<SettingsMcpServersParams>(output, config);
    push_decl::<SettingsMcpServersResult>(output, config);
    push_decl::<SettingsProjectionAvailability>(output, config);
    push_decl::<SettingsMcpServerRecord>(output, config);
    push_decl::<SettingsMcpServerTransport>(output, config);
    push_decl::<SettingsMcpServerStatus>(output, config);
    push_decl::<SettingsSkillsParams>(output, config);
    push_decl::<SettingsSkillsResult>(output, config);
    push_decl::<SettingsSkillRecord>(output, config);
    push_decl::<SettingsSkillStatus>(output, config);
    push_decl::<SettingsProjectionNotice>(output, config);
    push_decl::<SettingsProjectionNoticeSeverity>(output, config);
    push_decl::<SettingsScope>(output, config);
    push_decl::<AppPreferencesParams>(output, config);
    push_decl::<AppPreferencesUpdateParams>(output, config);
    push_decl::<AppPreferencesPatch>(output, config);
    push_decl::<AppPreferencesResult>(output, config);
    push_decl::<AppPreferences>(output, config);
    push_decl::<ComposerSubmitShortcut>(output, config);
    push_decl::<RuntimeSettingsParams>(output, config);
    push_decl::<RuntimeSettingsUpdateParams>(output, config);
    push_decl::<RuntimeDeveloperSettingsPatch>(output, config);
    push_decl::<RuntimeAcpTraceSettingsPatch>(output, config);
    push_decl::<RuntimeSettingsResult>(output, config);
    push_decl::<RuntimeDeveloperSettings>(output, config);
    push_decl::<RuntimeAcpTraceSettings>(output, config);

    push_decl::<WorkspaceListRootsParams>(output, config);
    push_decl::<WorkspaceListRootsResult>(output, config);
    push_decl::<WorkspaceBrowserRoot>(output, config);
    push_decl::<WorkspaceListDirectoryParams>(output, config);
    push_decl::<WorkspaceListDirectoryResult>(output, config);
    push_decl::<WorkspaceBrowserDirectory>(output, config);
    push_decl::<WorkspaceBrowserEntry>(output, config);

    push_decl::<AttachmentListRootsParams>(output, config);
    push_decl::<AttachmentListRootsResult>(output, config);
    push_decl::<FileBrowserRoot>(output, config);
    push_decl::<AttachmentListDirectoryParams>(output, config);
    push_decl::<AttachmentListDirectoryResult>(output, config);
    push_decl::<FileBrowserDirectory>(output, config);
    push_decl::<FileBrowserEntry>(output, config);
    push_decl::<FileBrowserEntryKind>(output, config);
    push_decl::<AttachmentCreateFileReferenceParams>(output, config);
    push_decl::<AttachmentCreateFileReferenceResult>(output, config);
    push_decl::<AttachmentCreatePastedImageParams>(output, config);
    push_decl::<AttachmentCreatePastedImageResult>(output, config);
    push_decl::<AttachmentCreateEmbeddedCandidateParams>(output, config);
    push_decl::<AttachmentCreateEmbeddedCandidateResult>(output, config);
    push_decl::<AttachmentConfirmEmbeddedParams>(output, config);
    push_decl::<AttachmentConfirmEmbeddedResult>(output, config);
    push_decl::<AttachmentRefreshHandlesParams>(output, config);
    push_decl::<AttachmentRefreshHandlesResult>(output, config);
    push_decl::<AttachmentResourceId>(output, config);
    push_decl::<AttachmentReleaseStatus>(output, config);
    push_decl::<AttachmentReleaseOutcome>(output, config);
    push_decl::<AttachmentReleaseParams>(output, config);
    push_decl::<AttachmentReleaseResult>(output, config);
    push_decl::<AttachmentRevealParams>(output, config);
    push_decl::<AttachmentRevealResult>(output, config);
    push_decl::<PreSendAttachment>(output, config);
    push_decl::<EmbeddedAttachmentCandidate>(output, config);
    push_decl::<AttachmentCandidateError>(output, config);
    push_decl::<AttachmentCandidateErrorCode>(output, config);

    push_decl::<PermissionRequestParams>(output, config);
    push_decl::<PendingRequestResolveParams>(output, config);
    push_decl::<PendingRequestResolution>(output, config);
    push_decl::<PendingRequestResolveResult>(output, config);
    push_decl::<PermissionToolCallRef>(output, config);
    push_decl::<PermissionRequestOption>(output, config);
    push_decl::<PermissionRequestOptionKind>(output, config);
    push_decl::<PermissionRequestResponse>(output, config);
    push_decl::<QuestionRequestParams>(output, config);
    push_decl::<QuestionField>(output, config);
    push_decl::<QuestionStringFormat>(output, config);
    push_decl::<QuestionOption>(output, config);
    push_decl::<QuestionRequestResponse>(output, config);
    push_decl::<QuestionValue>(output, config);
    push_decl::<SecretReadParams>(output, config);
    push_decl::<SecretReadResponse>(output, config);
    push_decl::<ShellShowNotificationParams>(output, config);
    push_decl::<ShellNotificationLevel>(output, config);
    push_decl::<ShellNotificationAction>(output, config);
    push_decl::<ShellShowNotificationResponse>(output, config);
    push_decl::<ShellRevealFileParams>(output, config);
    push_decl::<ShellRevealFileResponse>(output, config);
    push_decl::<ShellResolveFileRevealParams>(output, config);
    push_decl::<ShellResolveFileRevealResult>(output, config);

    push_decl::<TaskCreateParams>(output, config);
    push_decl::<TaskCreateResult>(output, config);
    push_decl::<TaskAdoptNativeSessionParams>(output, config);
    push_decl::<TaskAdoptNativeSessionResult>(output, config);
    push_decl::<TaskSendParams>(output, config);
    push_decl::<ComposerMessage>(output, config);
    push_decl::<TaskSendResult>(output, config);
    push_decl::<TaskSetConfigOptionParams>(output, config);
    push_decl::<TaskSetConfigOptionResult>(output, config);
    push_decl::<TaskCancelParams>(output, config);
    push_decl::<TaskCancelResult>(output, config);
    push_decl::<TaskChatPageParams>(output, config);
    push_decl::<TaskChatPageResult>(output, config);
    push_decl::<ToolDetailSnapshot>(output, config);
    push_decl::<ActivityToolLocation>(output, config);
    push_decl::<ActivityToolContent>(output, config);
    push_decl::<ActivityToolInput>(output, config);
    push_decl::<ActivityToolOutput>(output, config);
    push_decl::<ActivityToolField>(output, config);
    push_decl::<ActivityToolValue>(output, config);
    push_decl::<TaskOpenParams>(output, config);
    push_decl::<TaskOpenResult>(output, config);
    push_decl::<TaskMarkReadParams>(output, config);
    push_decl::<TaskMarkReadResult>(output, config);
    push_decl::<TaskListParams>(output, config);
    push_decl::<TaskListResult>(output, config);
    push_decl::<TaskDiscardParams>(output, config);
    push_decl::<TaskDiscardResult>(output, config);
    push_decl::<TaskSetArchivedParams>(output, config);
    push_decl::<TaskSetArchivedResult>(output, config);
    push_decl::<SupportRecoverStuckSessionsParams>(output, config);
    push_decl::<SupportRecoverStuckSessionsResult>(output, config);

    push_decl::<AppServerEvent>(output, config);
    push_decl::<EventScope>(output, config);
    push_decl::<AppServerEventPayload>(output, config);
    push_decl::<TaskChanges>(output, config);
    push_decl::<TaskChatChange>(output, config);
    push_decl::<TaskNavigationChange>(output, config);

    push_decl::<ClientSnapshot>(output, config);
    push_decl::<ServerSnapshot>(output, config);
    push_decl::<ProtocolVersion>(output, config);
    push_decl::<ServerCapabilities>(output, config);
    push_decl::<StateRootSnapshot>(output, config);
    push_decl::<ClientSnapshotScope>(output, config);
    push_decl::<NewTaskDefaultsSnapshot>(output, config);
    push_decl::<ProjectCollectionSnapshot>(output, config);
    push_decl::<ProjectSummary>(output, config);
    push_decl::<AgentCollectionSnapshot>(output, config);
    push_decl::<AgentSummary>(output, config);
    push_decl::<AgentStatus>(output, config);
    push_decl::<AgentCapabilities>(output, config);
    push_decl::<TaskNavigationSnapshot>(output, config);
    push_decl::<TaskSummary>(output, config);
    push_decl::<TaskTitle>(output, config);
    push_decl::<TaskTitleSource>(output, config);
    push_decl::<TaskStatus>(output, config);
    push_decl::<TaskLifecycle>(output, config);
    push_decl::<TaskSnapshot>(output, config);
    push_decl::<TaskHistorySyncSnapshot>(output, config);
    push_decl::<TaskPreparationSnapshot>(output, config);
    push_decl::<TaskPreparationStep>(output, config);
    push_decl::<TaskPreparationStepKind>(output, config);
    push_decl::<TaskPreparationStepStatus>(output, config);
    push_decl::<TaskSetupBlocker>(output, config);
    push_decl::<TaskSetupBlockerKind>(output, config);
    push_decl::<TaskPreparationAction>(output, config);
    push_decl::<TaskAgentConfigSnapshot>(output, config);
    push_decl::<AgentConfigOptionSnapshot>(output, config);
    push_decl::<AgentConfigOptionKind>(output, config);
    push_decl::<AgentConfigOptionValueSnapshot>(output, config);
    push_decl::<PendingAgentConfigChange>(output, config);
    push_decl::<TaskAgentCommandsSnapshot>(output, config);
    push_decl::<AgentSlashCommandSnapshot>(output, config);
    push_decl::<AgentSlashCommandInputSnapshot>(output, config);
    push_decl::<LiveSessionDataState>(output, config);
    push_decl::<TaskSendCapabilitySnapshot>(output, config);
    push_decl::<TaskSendCapabilityState>(output, config);
    push_decl::<TaskSendBlocker>(output, config);
    push_decl::<TaskSendBlockerKind>(output, config);
    push_decl::<ChatSnapshot>(output, config);
    push_decl::<ChatItem>(output, config);
    push_decl::<ChatRole>(output, config);
    push_decl::<ChatItemStatus>(output, config);
    push_decl::<MessagePart>(output, config);
    push_decl::<QuestionMessageState>(output, config);
    push_decl::<QuestionMessageAction>(output, config);
    push_decl::<ActivityStatus>(output, config);
    push_decl::<ActivityStepSnapshot>(output, config);
    push_decl::<ToolPermissionOutcomeSnapshot>(output, config);
    push_decl::<ToolPermissionDecisionSnapshot>(output, config);
    push_decl::<AttachmentSnapshot>(output, config);
    push_decl::<AttachmentKind>(output, config);
    push_decl::<RecoverySnapshot>(output, config);
    push_decl::<RecoveryAction>(output, config);
    push_decl::<SettingsSnapshot>(output, config);
    push_decl::<PendingRequestSnapshot>(output, config);
    push_decl::<PendingRequestScope>(output, config);
    push_decl::<PendingRequestKind>(output, config);
}

fn push_decl<T: TS + 'static>(output: &mut String, config: &Config) {
    output.push_str("export ");
    output.push_str(&T::decl(config));
    output.push_str("\n\n");
}
