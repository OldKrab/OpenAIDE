pub(super) fn push_method_maps(output: &mut String) {
    output.push_str("export type ProtocolMethod = typeof CLIENT_PROBE | typeof CLIENT_INITIALIZE | typeof CLIENT_CAPABILITIES_CHANGED | typeof CLIENT_HEARTBEAT | typeof STATE_SUBSCRIBE | typeof STATE_UNSUBSCRIBE | typeof DIAGNOSTICS_GET_RUNTIME | typeof SUPPORT_RECOVER_STUCK_SESSIONS | typeof AGENT_PROBE | typeof AGENT_AUTHENTICATE | typeof AGENT_LIST_SESSIONS | typeof AGENT_CREATE_CUSTOM | typeof AGENT_UPDATE_CUSTOM_METADATA | typeof AGENT_REPLACE_CUSTOM | typeof AGENT_DELETE_CUSTOM | typeof AGENT_SET_ENABLED | typeof SETTINGS_GET_AGENT_DETAILS | typeof SETTINGS_GET_MCP_SERVERS | typeof SETTINGS_GET_SKILLS | typeof SETTINGS_GET_PREFERENCES | typeof SETTINGS_UPDATE_PREFERENCES | typeof SETTINGS_GET_RUNTIME | typeof SETTINGS_UPDATE_RUNTIME | typeof ATTACHMENT_LIST_ROOTS | typeof ATTACHMENT_LIST_DIRECTORY | typeof ATTACHMENT_CREATE_FILE_REFERENCE | typeof ATTACHMENT_CREATE_PASTED_IMAGE | typeof ATTACHMENT_CREATE_EMBEDDED_CANDIDATE | typeof ATTACHMENT_CONFIRM_EMBEDDED | typeof ATTACHMENT_REFRESH_HANDLES | typeof ATTACHMENT_RELEASE | typeof ATTACHMENT_REVEAL | typeof SHELL_RESOLVE_FILE_REVEAL | typeof WORKSPACE_LIST_ROOTS | typeof WORKSPACE_LIST_DIRECTORY | typeof TASK_CREATE | typeof TASK_ADOPT_NATIVE_SESSION | typeof TASK_SEND | typeof TASK_SET_CONFIG_OPTION | typeof TASK_CANCEL | typeof TASK_OPEN | typeof TASK_RETRY_HISTORY_SYNC | typeof TASK_MARK_READ | typeof TASK_CHAT_PAGE | typeof TASK_TOOL_DETAIL | typeof TASK_LIST | typeof TASK_DISCARD | typeof TASK_SET_ARCHIVED;\n");
    output.push_str("export type RequestParamsByMethod = {\n");
    output.push_str("  [CLIENT_PROBE]: ClientProbeParams;\n");
    output.push_str("  [CLIENT_INITIALIZE]: InitializeParams;\n");
    output.push_str("  [CLIENT_CAPABILITIES_CHANGED]: ClientCapabilitiesChangedParams;\n");
    output.push_str("  [CLIENT_HEARTBEAT]: ClientHeartbeatParams;\n");
    output.push_str("  [STATE_SUBSCRIBE]: StateSubscribeParams;\n");
    output.push_str("  [STATE_UNSUBSCRIBE]: StateUnsubscribeParams;\n");
    output.push_str("  [DIAGNOSTICS_GET_RUNTIME]: RuntimeDiagnosticsParams;\n");
    output.push_str("  [SUPPORT_RECOVER_STUCK_SESSIONS]: SupportRecoverStuckSessionsParams;\n");
    output.push_str("  [AGENT_PROBE]: AgentProbeParams;\n");
    output.push_str("  [AGENT_AUTHENTICATE]: AgentAuthenticateParams;\n");
    output.push_str("  [AGENT_LIST_SESSIONS]: AgentListSessionsParams;\n");
    output.push_str("  [AGENT_CREATE_CUSTOM]: AgentCreateCustomParams;\n");
    output.push_str("  [AGENT_UPDATE_CUSTOM_METADATA]: AgentUpdateCustomMetadataParams;\n");
    output.push_str("  [AGENT_REPLACE_CUSTOM]: AgentReplaceCustomParams;\n");
    output.push_str("  [AGENT_DELETE_CUSTOM]: AgentDeleteCustomParams;\n");
    output.push_str("  [AGENT_SET_ENABLED]: AgentSetEnabledParams;\n");
    output.push_str("  [SETTINGS_GET_AGENT_DETAILS]: AgentSettingsDetailsParams;\n");
    output.push_str("  [SETTINGS_GET_MCP_SERVERS]: SettingsMcpServersParams;\n");
    output.push_str("  [SETTINGS_GET_SKILLS]: SettingsSkillsParams;\n");
    output.push_str("  [SETTINGS_GET_PREFERENCES]: AppPreferencesParams;\n");
    output.push_str("  [SETTINGS_UPDATE_PREFERENCES]: AppPreferencesUpdateParams;\n");
    output.push_str("  [SETTINGS_GET_RUNTIME]: RuntimeSettingsParams;\n");
    output.push_str("  [SETTINGS_UPDATE_RUNTIME]: RuntimeSettingsUpdateParams;\n");
    output.push_str("  [ATTACHMENT_LIST_ROOTS]: AttachmentListRootsParams;\n");
    output.push_str("  [ATTACHMENT_LIST_DIRECTORY]: AttachmentListDirectoryParams;\n");
    output.push_str("  [ATTACHMENT_CREATE_FILE_REFERENCE]: AttachmentCreateFileReferenceParams;\n");
    output.push_str("  [ATTACHMENT_CREATE_PASTED_IMAGE]: AttachmentCreatePastedImageParams;\n");
    output.push_str(
        "  [ATTACHMENT_CREATE_EMBEDDED_CANDIDATE]: AttachmentCreateEmbeddedCandidateParams;\n",
    );
    output.push_str("  [ATTACHMENT_CONFIRM_EMBEDDED]: AttachmentConfirmEmbeddedParams;\n");
    output.push_str("  [ATTACHMENT_REFRESH_HANDLES]: AttachmentRefreshHandlesParams;\n");
    output.push_str("  [ATTACHMENT_RELEASE]: AttachmentReleaseParams;\n");
    output.push_str("  [ATTACHMENT_REVEAL]: AttachmentRevealParams;\n");
    output.push_str("  [SHELL_RESOLVE_FILE_REVEAL]: ShellResolveFileRevealParams;\n");
    output.push_str("  [WORKSPACE_LIST_ROOTS]: WorkspaceListRootsParams;\n");
    output.push_str("  [WORKSPACE_LIST_DIRECTORY]: WorkspaceListDirectoryParams;\n");
    output.push_str("  [TASK_CREATE]: TaskCreateParams;\n");
    output.push_str("  [TASK_ADOPT_NATIVE_SESSION]: TaskAdoptNativeSessionParams;\n");
    output.push_str("  [TASK_SEND]: TaskSendParams;\n");
    output.push_str("  [TASK_SET_CONFIG_OPTION]: TaskSetConfigOptionParams;\n");
    output.push_str("  [TASK_CANCEL]: TaskCancelParams;\n");
    output.push_str("  [TASK_OPEN]: TaskOpenParams;\n");
    output.push_str("  [TASK_RETRY_HISTORY_SYNC]: TaskRetryHistorySyncParams;\n");
    output.push_str("  [TASK_MARK_READ]: TaskMarkReadParams;\n");
    output.push_str("  [TASK_CHAT_PAGE]: TaskChatPageParams;\n");
    output.push_str("  [TASK_TOOL_DETAIL]: TaskToolDetailParams;\n");
    output.push_str("  [TASK_LIST]: TaskListParams;\n");
    output.push_str("  [TASK_DISCARD]: TaskDiscardParams;\n");
    output.push_str("  [TASK_SET_ARCHIVED]: TaskSetArchivedParams;\n");
    output.push_str("};\n\n");
    output.push_str("export type ResponseResultByMethod = {\n");
    output.push_str("  [CLIENT_PROBE]: ClientProbeResult;\n");
    output.push_str("  [CLIENT_INITIALIZE]: InitializeResult;\n");
    output.push_str("  [CLIENT_CAPABILITIES_CHANGED]: ClientCapabilitiesChangedResult;\n");
    output.push_str("  [CLIENT_HEARTBEAT]: ClientHeartbeatResult;\n");
    output.push_str("  [STATE_SUBSCRIBE]: StateSubscribeResult;\n");
    output.push_str("  [STATE_UNSUBSCRIBE]: StateUnsubscribeResult;\n");
    output.push_str("  [DIAGNOSTICS_GET_RUNTIME]: RuntimeDiagnosticsResult;\n");
    output.push_str("  [SUPPORT_RECOVER_STUCK_SESSIONS]: SupportRecoverStuckSessionsResult;\n");
    output.push_str("  [AGENT_PROBE]: AgentProbeResult;\n");
    output.push_str("  [AGENT_AUTHENTICATE]: AgentAuthenticateResult;\n");
    output.push_str("  [AGENT_LIST_SESSIONS]: AgentListSessionsResult;\n");
    output.push_str("  [AGENT_CREATE_CUSTOM]: AgentCreateCustomResult;\n");
    output.push_str("  [AGENT_UPDATE_CUSTOM_METADATA]: AgentUpdateCustomMetadataResult;\n");
    output.push_str("  [AGENT_REPLACE_CUSTOM]: AgentReplaceCustomResult;\n");
    output.push_str("  [AGENT_DELETE_CUSTOM]: AgentDeleteCustomResult;\n");
    output.push_str("  [AGENT_SET_ENABLED]: AgentSetEnabledResult;\n");
    output.push_str("  [SETTINGS_GET_AGENT_DETAILS]: AgentSettingsDetailsResult;\n");
    output.push_str("  [SETTINGS_GET_MCP_SERVERS]: SettingsMcpServersResult;\n");
    output.push_str("  [SETTINGS_GET_SKILLS]: SettingsSkillsResult;\n");
    output.push_str("  [SETTINGS_GET_PREFERENCES]: AppPreferencesResult;\n");
    output.push_str("  [SETTINGS_UPDATE_PREFERENCES]: AppPreferencesResult;\n");
    output.push_str("  [SETTINGS_GET_RUNTIME]: RuntimeSettingsResult;\n");
    output.push_str("  [SETTINGS_UPDATE_RUNTIME]: RuntimeSettingsResult;\n");
    output.push_str("  [ATTACHMENT_LIST_ROOTS]: AttachmentListRootsResult;\n");
    output.push_str("  [ATTACHMENT_LIST_DIRECTORY]: AttachmentListDirectoryResult;\n");
    output.push_str("  [ATTACHMENT_CREATE_FILE_REFERENCE]: AttachmentCreateFileReferenceResult;\n");
    output.push_str("  [ATTACHMENT_CREATE_PASTED_IMAGE]: AttachmentCreatePastedImageResult;\n");
    output.push_str(
        "  [ATTACHMENT_CREATE_EMBEDDED_CANDIDATE]: AttachmentCreateEmbeddedCandidateResult;\n",
    );
    output.push_str("  [ATTACHMENT_CONFIRM_EMBEDDED]: AttachmentConfirmEmbeddedResult;\n");
    output.push_str("  [ATTACHMENT_REFRESH_HANDLES]: AttachmentRefreshHandlesResult;\n");
    output.push_str("  [ATTACHMENT_RELEASE]: AttachmentReleaseResult;\n");
    output.push_str("  [ATTACHMENT_REVEAL]: AttachmentRevealResult;\n");
    output.push_str("  [SHELL_RESOLVE_FILE_REVEAL]: ShellResolveFileRevealResult;\n");
    output.push_str("  [WORKSPACE_LIST_ROOTS]: WorkspaceListRootsResult;\n");
    output.push_str("  [WORKSPACE_LIST_DIRECTORY]: WorkspaceListDirectoryResult;\n");
    output.push_str("  [TASK_CREATE]: TaskCreateResult;\n");
    output.push_str("  [TASK_ADOPT_NATIVE_SESSION]: TaskAdoptNativeSessionResult;\n");
    output.push_str("  [TASK_SEND]: TaskSendResult;\n");
    output.push_str("  [TASK_SET_CONFIG_OPTION]: TaskSetConfigOptionResult;\n");
    output.push_str("  [TASK_CANCEL]: TaskCancelResult;\n");
    output.push_str("  [TASK_OPEN]: TaskOpenResult;\n");
    output.push_str("  [TASK_RETRY_HISTORY_SYNC]: TaskRetryHistorySyncResult;\n");
    output.push_str("  [TASK_MARK_READ]: TaskMarkReadResult;\n");
    output.push_str("  [TASK_CHAT_PAGE]: TaskChatPageResult;\n");
    output.push_str("  [TASK_TOOL_DETAIL]: TaskToolDetailResult;\n");
    output.push_str("  [TASK_LIST]: TaskListResult;\n");
    output.push_str("  [TASK_DISCARD]: TaskDiscardResult;\n");
    output.push_str("  [TASK_SET_ARCHIVED]: TaskSetArchivedResult;\n");
    output.push_str("};\n\n");
    output.push_str("export type TypedClientRequest<M extends ProtocolMethod> = ClientRequestEnvelope<RequestParamsByMethod[M]> & {\n");
    output.push_str("  method: M;\n");
    output.push_str("};\n\n");
    output.push_str("export type TypedClientResponse<M extends ProtocolMethod> = ResponseEnvelope<ResponseResultByMethod[M]>;\n\n");
    output.push_str("export type ClientProbeRequest = TypedClientRequest<typeof CLIENT_PROBE>;\n");
    output.push_str("export type ClientProbeResponse = ResponseEnvelope<ClientProbeResult>;\n");
    output.push_str(
        "export type ClientInitializeRequest = TypedClientRequest<typeof CLIENT_INITIALIZE>;\n",
    );
    output.push_str(
        "export type ClientHeartbeatRequest = TypedClientRequest<typeof CLIENT_HEARTBEAT>;\n",
    );
    output.push_str(
        "export type ClientCapabilitiesChangedRequest = TypedClientRequest<typeof CLIENT_CAPABILITIES_CHANGED>;\n",
    );
    output.push_str("export type ClientInitializeResponse = ResponseEnvelope<InitializeResult>;\n");
    output.push_str(
        "export type ClientCapabilitiesChangedResponse = ResponseEnvelope<ClientCapabilitiesChangedResult>;\n",
    );
    output.push_str(
        "export type ClientHeartbeatResponse = ResponseEnvelope<ClientHeartbeatResult>;\n",
    );
    output
        .push_str("export type StateSubscribeResponse = ResponseEnvelope<StateSubscribeResult>;\n");
    output.push_str(
        "export type StateUnsubscribeResponse = ResponseEnvelope<StateUnsubscribeResult>;\n",
    );
    output.push_str(
        "export type DiagnosticsGetRuntimeResponse = ResponseEnvelope<RuntimeDiagnosticsResult>;\n",
    );
    output.push_str(
        "export type SupportRecoverStuckSessionsResponse = ResponseEnvelope<SupportRecoverStuckSessionsResult>;\n",
    );
    output.push_str("export type AgentProbeResponse = ResponseEnvelope<AgentProbeResult>;\n");
    output.push_str(
        "export type AgentAuthenticateResponse = ResponseEnvelope<AgentAuthenticateResult>;\n",
    );
    output.push_str(
        "export type AgentListSessionsResponse = ResponseEnvelope<AgentListSessionsResult>;\n",
    );
    output.push_str(
        "export type AgentCreateCustomResponse = ResponseEnvelope<AgentCreateCustomResult>;\n",
    );
    output.push_str(
        "export type AgentUpdateCustomMetadataResponse = ResponseEnvelope<AgentUpdateCustomMetadataResult>;\n",
    );
    output.push_str(
        "export type AgentReplaceCustomResponse = ResponseEnvelope<AgentReplaceCustomResult>;\n",
    );
    output.push_str(
        "export type AgentDeleteCustomResponse = ResponseEnvelope<AgentDeleteCustomResult>;\n",
    );
    output.push_str(
        "export type AgentSetEnabledResponse = ResponseEnvelope<AgentSetEnabledResult>;\n",
    );
    output.push_str(
        "export type SettingsGetAgentDetailsResponse = ResponseEnvelope<AgentSettingsDetailsResult>;\n",
    );
    output.push_str(
        "export type SettingsGetMcpServersResponse = ResponseEnvelope<SettingsMcpServersResult>;\n",
    );
    output.push_str(
        "export type SettingsGetSkillsResponse = ResponseEnvelope<SettingsSkillsResult>;\n",
    );
    output.push_str(
        "export type SettingsGetPreferencesResponse = ResponseEnvelope<AppPreferencesResult>;\n",
    );
    output.push_str(
        "export type SettingsUpdatePreferencesResponse = ResponseEnvelope<AppPreferencesResult>;\n",
    );
    output.push_str(
        "export type SettingsGetRuntimeResponse = ResponseEnvelope<RuntimeSettingsResult>;\n",
    );
    output.push_str(
        "export type SettingsUpdateRuntimeResponse = ResponseEnvelope<RuntimeSettingsResult>;\n",
    );
    output.push_str(
        "export type AttachmentListRootsResponse = ResponseEnvelope<AttachmentListRootsResult>;\n",
    );
    output.push_str(
        "export type AttachmentListDirectoryResponse = ResponseEnvelope<AttachmentListDirectoryResult>;\n",
    );
    output.push_str(
        "export type AttachmentCreateFileReferenceResponse = ResponseEnvelope<AttachmentCreateFileReferenceResult>;\n",
    );
    output.push_str(
        "export type AttachmentCreatePastedImageResponse = ResponseEnvelope<AttachmentCreatePastedImageResult>;\n",
    );
    output.push_str(
        "export type AttachmentCreateEmbeddedCandidateResponse = ResponseEnvelope<AttachmentCreateEmbeddedCandidateResult>;\n",
    );
    output.push_str(
        "export type AttachmentConfirmEmbeddedResponse = ResponseEnvelope<AttachmentConfirmEmbeddedResult>;\n",
    );
    output.push_str(
        "export type AttachmentRefreshHandlesResponse = ResponseEnvelope<AttachmentRefreshHandlesResult>;\n",
    );
    output.push_str(
        "export type AttachmentReleaseResponse = ResponseEnvelope<AttachmentReleaseResult>;\n",
    );
    output.push_str(
        "export type AttachmentRevealResponse = ResponseEnvelope<AttachmentRevealResult>;\n",
    );
    output.push_str(
        "export type WorkspaceListRootsResponse = ResponseEnvelope<WorkspaceListRootsResult>;\n",
    );
    output.push_str(
        "export type WorkspaceListDirectoryResponse = ResponseEnvelope<WorkspaceListDirectoryResult>;\n",
    );
    output.push_str("export type TaskCreateResponse = ResponseEnvelope<TaskCreateResult>;\n");
    output.push_str(
        "export type TaskAdoptNativeSessionResponse = ResponseEnvelope<TaskAdoptNativeSessionResult>;\n",
    );
    output.push_str("export type TaskSendResponse = ResponseEnvelope<TaskSendResult>;\n");
    output.push_str(
        "export type TaskSetConfigOptionResponse = ResponseEnvelope<TaskSetConfigOptionResult>;\n",
    );
    output.push_str("export type TaskCancelResponse = ResponseEnvelope<TaskCancelResult>;\n");
    output.push_str("export type TaskOpenResponse = ResponseEnvelope<TaskOpenResult>;\n");
    output.push_str("export type TaskChatPageResponse = ResponseEnvelope<TaskChatPageResult>;\n");
    output
        .push_str("export type TaskToolDetailResponse = ResponseEnvelope<TaskToolDetailResult>;\n");
    output.push_str("export type TaskListResponse = ResponseEnvelope<TaskListResult>;\n");
    output.push_str("export type TaskDiscardResponse = ResponseEnvelope<TaskDiscardResult>;\n");
    output.push_str(
        "export type TaskSetArchivedResponse = ResponseEnvelope<TaskSetArchivedResult>;\n",
    );
    output.push('\n');
    output.push_str("export type ServerRequestMethod = typeof PERMISSION_REQUEST | typeof QUESTION_REQUEST | typeof SECRET_READ | typeof SHELL_SHOW_NOTIFICATION | typeof SHELL_REVEAL_FILE;\n");
    output.push_str("export type ServerRequestParamsByMethod = {\n");
    output.push_str("  [PERMISSION_REQUEST]: PermissionRequestParams;\n");
    output.push_str("  [QUESTION_REQUEST]: QuestionRequestParams;\n");
    output.push_str("  [SECRET_READ]: SecretReadParams;\n");
    output.push_str("  [SHELL_SHOW_NOTIFICATION]: ShellShowNotificationParams;\n");
    output.push_str("  [SHELL_REVEAL_FILE]: ShellRevealFileParams;\n");
    output.push_str("};\n\n");
    output.push_str("export type ServerRequestResponseResultByMethod = {\n");
    output.push_str("  [PERMISSION_REQUEST]: PermissionRequestResponse;\n");
    output.push_str("  [QUESTION_REQUEST]: QuestionRequestResponse;\n");
    output.push_str("  [SECRET_READ]: SecretReadResponse;\n");
    output.push_str("  [SHELL_SHOW_NOTIFICATION]: ShellShowNotificationResponse;\n");
    output.push_str("  [SHELL_REVEAL_FILE]: ShellRevealFileResponse;\n");
    output.push_str("};\n\n");
    output.push_str("export type TypedServerRequest<M extends ServerRequestMethod> = ServerRequestEnvelope<ServerRequestParamsByMethod[M]> & {\n");
    output.push_str("  method: M;\n");
    output.push_str("};\n");
}
