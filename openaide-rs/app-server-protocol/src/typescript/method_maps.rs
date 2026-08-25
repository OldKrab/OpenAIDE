pub(super) fn push_method_maps(output: &mut String) {
    output.push_str("export type ProtocolMethod = typeof CLIENT_PROBE | typeof CLIENT_INITIALIZE | typeof CLIENT_CAPABILITIES_CHANGED | typeof CLIENT_HEARTBEAT | typeof CLIENT_DETACH | typeof PENDING_REQUEST_RESOLVE | typeof STATE_SUBSCRIBE | typeof STATE_UNSUBSCRIBE | typeof DIAGNOSTICS_GET_RUNTIME | typeof SUPPORT_RECOVER_STUCK_SESSIONS | typeof AGENT_PROBE | typeof AGENT_AUTHENTICATE | typeof AGENT_LIST_SESSIONS | typeof AGENT_CREATE_CUSTOM | typeof AGENT_UPDATE_CUSTOM_METADATA | typeof AGENT_REPLACE_CUSTOM | typeof AGENT_DELETE_CUSTOM | typeof AGENT_SET_ENABLED | typeof SETTINGS_GET_AGENT_DETAILS | typeof SETTINGS_GET_MCP_SERVERS | typeof MCP_GET_SERVER_DETAILS | typeof MCP_CREATE_SERVER | typeof MCP_UPDATE_SERVER | typeof MCP_DELETE_SERVER | typeof MCP_SET_SERVER_ENABLED | typeof SETTINGS_GET_SKILLS | typeof SETTINGS_GET_SKILL_DETAILS | typeof SETTINGS_GET_PREFERENCES | typeof SETTINGS_UPDATE_PREFERENCES | typeof SETTINGS_UPDATE_NEW_TASK_DEFAULTS | typeof SETTINGS_GET_RUNTIME | typeof SETTINGS_UPDATE_RUNTIME | typeof ATTACHMENT_LIST_ROOTS | typeof ATTACHMENT_LIST_DIRECTORY | typeof ATTACHMENT_CREATE_FILE_REFERENCE | typeof ATTACHMENT_CREATE_LOCAL_FILE_REFERENCES | typeof ATTACHMENT_CREATE_PASTED_IMAGE | typeof ATTACHMENT_CREATE_EMBEDDED_CANDIDATE | typeof ATTACHMENT_CONFIRM_EMBEDDED | typeof ATTACHMENT_REFRESH_HANDLES | typeof ATTACHMENT_RELEASE | typeof ATTACHMENT_REVEAL | typeof ATTACHMENT_REVEAL_SENT | typeof SHELL_RESOLVE_FILE_REVEAL | typeof WORKSPACE_LIST_ROOTS | typeof WORKSPACE_LIST_DIRECTORY | typeof WORKTREE_REFRESH | typeof WORKTREE_CREATE | typeof WORKTREE_RECREATE | typeof WORKTREE_REMOVAL_PREFLIGHT | typeof WORKTREE_REMOVE | typeof WORKTREE_RENAME | typeof WORKTREE_RESOLVE_FOLDER | typeof WORKTREE_LINKED_TASKS | typeof TASK_ACQUIRE | typeof TASK_ACQUIRE_IN_WORKTREE | typeof TASK_SEARCH_FILES | typeof TASK_ADOPT_NATIVE_SESSION | typeof TASK_SEND | typeof TASK_SET_CONFIG_OPTION | typeof TASK_SET_TITLE | typeof TASK_CANCEL | typeof TASK_OPEN | typeof TASK_MARK_READ | typeof TASK_CHAT_PAGE | typeof TASK_LIST | typeof TASK_NAVIGATION_REFRESH | typeof TASK_NAVIGATION_LOAD_MORE | typeof NATIVE_SESSION_ARCHIVE | typeof NATIVE_SESSION_SET_TITLE | typeof NATIVE_SESSION_SET_PINNED | typeof NATIVE_SESSION_RESTORE | typeof TASK_RELEASE | typeof TASK_ARCHIVE | typeof TASK_RESTORE;\n");
    // Append new methods outside the legacy monolithic literal so additions remain reviewable.
    let method_union_end = output.len() - ";\n".len();
    output.insert_str(
        method_union_end,
        " | typeof DIAGNOSTICS_LIST_SUPPORT_EXPORT | typeof DIAGNOSTICS_CREATE_SUPPORT_EXPORT | typeof PROJECT_ADD | typeof PROJECT_RENAME | typeof PROJECT_REMOVE | typeof PROJECT_REFRESH | typeof TASK_QUEUE_APPEND | typeof TASK_QUEUE_REMOVE | typeof TASK_QUEUE_TAKE | typeof TASK_QUEUE_MOVE | typeof TASK_SET_PERMISSION_POLICY | typeof TASK_SET_PINNED | typeof TASK_CLOSE_PLAN | typeof TASK_TOOL_IMAGE_PREVIEW | typeof FILE_VIEWER_OPEN | typeof FILE_VIEWER_OPEN_FROM_HANDLE | typeof FILE_VIEWER_REFRESH | typeof FILE_VIEWER_RELEASE | typeof TASK_COMPOSER_HISTORY | typeof SETTINGS_RESET_TASK_HISTORY | typeof NATIVE_SESSION_FORK | typeof TASK_RELOAD_NATIVE_SESSION | typeof TASK_ARCHIVE_OLDER",
    );
    output.push_str("export type RequestParamsByMethod = {\n");
    output.push_str("  [CLIENT_PROBE]: ClientProbeParams;\n");
    output.push_str("  [CLIENT_INITIALIZE]: InitializeParams;\n");
    output.push_str("  [CLIENT_CAPABILITIES_CHANGED]: ClientCapabilitiesChangedParams;\n");
    output.push_str("  [CLIENT_HEARTBEAT]: ClientHeartbeatParams;\n");
    output.push_str("  [CLIENT_DETACH]: ClientDetachParams;\n");
    output.push_str("  [PENDING_REQUEST_RESOLVE]: PendingRequestResolveParams;\n");
    output.push_str("  [STATE_SUBSCRIBE]: StateSubscribeParams;\n");
    output.push_str("  [STATE_UNSUBSCRIBE]: StateUnsubscribeParams;\n");
    output.push_str("  [DIAGNOSTICS_GET_RUNTIME]: RuntimeDiagnosticsParams;\n");
    output.push_str("  [DIAGNOSTICS_LIST_SUPPORT_EXPORT]: SupportExportListParams;\n");
    output.push_str("  [DIAGNOSTICS_CREATE_SUPPORT_EXPORT]: SupportExportCreateParams;\n");
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
    output.push_str("  [MCP_GET_SERVER_DETAILS]: McpGetServerDetailsParams;\n");
    output.push_str("  [MCP_CREATE_SERVER]: McpCreateServerParams;\n");
    output.push_str("  [MCP_UPDATE_SERVER]: McpUpdateServerParams;\n");
    output.push_str("  [MCP_DELETE_SERVER]: McpDeleteServerParams;\n");
    output.push_str("  [MCP_SET_SERVER_ENABLED]: McpSetServerEnabledParams;\n");
    output.push_str("  [SETTINGS_GET_SKILLS]: SettingsSkillsParams;\n");
    output.push_str("  [SETTINGS_GET_SKILL_DETAILS]: SettingsSkillDetailsParams;\n");
    output.push_str("  [SETTINGS_GET_PREFERENCES]: AppPreferencesParams;\n");
    output.push_str("  [SETTINGS_UPDATE_PREFERENCES]: AppPreferencesUpdateParams;\n");
    output.push_str("  [SETTINGS_UPDATE_NEW_TASK_DEFAULTS]: NewTaskDefaultsUpdateParams;\n");
    output.push_str("  [SETTINGS_GET_RUNTIME]: RuntimeSettingsParams;\n");
    output.push_str("  [SETTINGS_UPDATE_RUNTIME]: RuntimeSettingsUpdateParams;\n");
    output.push_str("  [SETTINGS_RESET_TASK_HISTORY]: ResetTaskHistoryParams;\n");
    output.push_str("  [ATTACHMENT_LIST_ROOTS]: AttachmentListRootsParams;\n");
    output.push_str("  [ATTACHMENT_LIST_DIRECTORY]: AttachmentListDirectoryParams;\n");
    output.push_str("  [ATTACHMENT_CREATE_FILE_REFERENCE]: AttachmentCreateFileReferenceParams;\n");
    output.push_str(
        "  [ATTACHMENT_CREATE_LOCAL_FILE_REFERENCES]: AttachmentCreateLocalFileReferencesParams;\n",
    );
    output.push_str("  [ATTACHMENT_CREATE_PASTED_IMAGE]: AttachmentCreatePastedImageParams;\n");
    output.push_str(
        "  [ATTACHMENT_CREATE_EMBEDDED_CANDIDATE]: AttachmentCreateEmbeddedCandidateParams;\n",
    );
    output.push_str("  [ATTACHMENT_CONFIRM_EMBEDDED]: AttachmentConfirmEmbeddedParams;\n");
    output.push_str("  [ATTACHMENT_REFRESH_HANDLES]: AttachmentRefreshHandlesParams;\n");
    output.push_str("  [ATTACHMENT_RELEASE]: AttachmentReleaseParams;\n");
    output.push_str("  [ATTACHMENT_REVEAL]: AttachmentRevealParams;\n");
    output.push_str("  [ATTACHMENT_REVEAL_SENT]: AttachmentRevealSentParams;\n");
    output.push_str("  [SHELL_RESOLVE_FILE_REVEAL]: ShellResolveFileRevealParams;\n");
    output.push_str("  [WORKSPACE_LIST_ROOTS]: WorkspaceListRootsParams;\n");
    output.push_str("  [WORKSPACE_LIST_DIRECTORY]: WorkspaceListDirectoryParams;\n");
    output.push_str("  [PROJECT_ADD]: ProjectAddParams;\n");
    output.push_str("  [PROJECT_RENAME]: ProjectRenameParams;\n");
    output.push_str("  [PROJECT_REMOVE]: ProjectRemoveParams;\n");
    output.push_str("  [PROJECT_REFRESH]: ProjectRefreshParams;\n");
    output.push_str("  [WORKTREE_REFRESH]: WorktreeRefreshParams;\n");
    output.push_str("  [WORKTREE_CREATE]: WorktreeCreateParams;\n");
    output.push_str("  [WORKTREE_RECREATE]: WorktreeRecreateParams;\n");
    output.push_str("  [WORKTREE_REMOVAL_PREFLIGHT]: WorktreeRemovalPreflightParams;\n");
    output.push_str("  [WORKTREE_REMOVE]: WorktreeRemoveParams;\n");
    output.push_str("  [WORKTREE_RENAME]: WorktreeRenameParams;\n");
    output.push_str("  [WORKTREE_RESOLVE_FOLDER]: WorktreeResolveFolderParams;\n");
    output.push_str("  [WORKTREE_LINKED_TASKS]: WorktreeLinkedTasksParams;\n");
    output.push_str("  [TASK_ACQUIRE]: TaskAcquireParams;\n");
    output.push_str("  [TASK_ACQUIRE_IN_WORKTREE]: TaskAcquireInWorktreeParams;\n");
    output.push_str("  [TASK_SEARCH_FILES]: TaskSearchFilesParams;\n");
    output.push_str("  [TASK_ADOPT_NATIVE_SESSION]: TaskAdoptNativeSessionParams;\n");
    output.push_str("  [TASK_SEND]: TaskSendParams;\n");
    output.push_str("  [TASK_QUEUE_APPEND]: TaskQueueAppendParams;\n");
    output.push_str("  [TASK_QUEUE_REMOVE]: TaskQueueRemoveParams;\n");
    output.push_str("  [TASK_QUEUE_TAKE]: TaskQueueTakeParams;\n");
    output.push_str("  [TASK_QUEUE_MOVE]: TaskQueueMoveParams;\n");
    output.push_str("  [TASK_SET_CONFIG_OPTION]: TaskSetConfigOptionParams;\n");
    output.push_str("  [TASK_SET_PERMISSION_POLICY]: TaskSetPermissionPolicyParams;\n");
    output.push_str("  [TASK_SET_TITLE]: TaskSetTitleParams;\n");
    output.push_str("  [TASK_SET_PINNED]: TaskSetPinnedParams;\n");
    output.push_str("  [TASK_CLOSE_PLAN]: TaskClosePlanParams;\n");
    output.push_str("  [TASK_TOOL_IMAGE_PREVIEW]: TaskToolImagePreviewParams;\n");
    output.push_str("  [FILE_VIEWER_OPEN]: FileViewerOpenParams;\n");
    output.push_str("  [FILE_VIEWER_OPEN_FROM_HANDLE]: FileViewerOpenFromHandleParams;\n");
    output.push_str("  [FILE_VIEWER_REFRESH]: FileViewerRefreshParams;\n");
    output.push_str("  [FILE_VIEWER_RELEASE]: FileViewerReleaseParams;\n");
    output.push_str("  [TASK_CANCEL]: TaskCancelParams;\n");
    output.push_str("  [TASK_OPEN]: TaskOpenParams;\n");
    output.push_str("  [TASK_RELOAD_NATIVE_SESSION]: TaskReloadNativeSessionParams;\n");
    output.push_str("  [TASK_MARK_READ]: TaskMarkReadParams;\n");
    output.push_str("  [TASK_CHAT_PAGE]: TaskChatPageParams;\n");
    output.push_str("  [TASK_COMPOSER_HISTORY]: ComposerHistoryParams;\n");
    output.push_str("  [TASK_LIST]: TaskListParams;\n");
    output.push_str("  [TASK_NAVIGATION_REFRESH]: TaskNavigationRefreshParams;\n");
    output.push_str("  [TASK_NAVIGATION_LOAD_MORE]: TaskNavigationLoadMoreParams;\n");
    output.push_str("  [NATIVE_SESSION_ARCHIVE]: NativeSessionArchiveParams;\n");
    output.push_str("  [NATIVE_SESSION_SET_TITLE]: NativeSessionSetTitleParams;\n");
    output.push_str("  [NATIVE_SESSION_SET_PINNED]: NativeSessionSetPinnedParams;\n");
    output.push_str("  [NATIVE_SESSION_RESTORE]: NativeSessionRestoreParams;\n");
    output.push_str("  [NATIVE_SESSION_FORK]: NativeSessionForkParams;\n");
    output.push_str("  [TASK_RELEASE]: TaskReleaseParams;\n");
    output.push_str("  [TASK_ARCHIVE]: TaskArchiveParams;\n  [TASK_ARCHIVE_OLDER]: TaskArchiveOlderParams;\n  [TASK_RESTORE]: TaskRestoreParams;\n");
    output.push_str("};\n\n");
    output.push_str("export type ResponseResultByMethod = {\n");
    output.push_str("  [CLIENT_PROBE]: ClientProbeResult;\n");
    output.push_str("  [CLIENT_INITIALIZE]: InitializeResult;\n");
    output.push_str("  [CLIENT_CAPABILITIES_CHANGED]: ClientCapabilitiesChangedResult;\n");
    output.push_str("  [CLIENT_HEARTBEAT]: ClientHeartbeatResult;\n");
    output.push_str("  [CLIENT_DETACH]: ClientDetachResult;\n");
    output.push_str("  [PENDING_REQUEST_RESOLVE]: PendingRequestResolveResult;\n");
    output.push_str("  [STATE_SUBSCRIBE]: StateSubscribeResult;\n");
    output.push_str("  [STATE_UNSUBSCRIBE]: StateUnsubscribeResult;\n");
    output.push_str("  [DIAGNOSTICS_GET_RUNTIME]: RuntimeDiagnosticsResult;\n");
    output.push_str("  [DIAGNOSTICS_LIST_SUPPORT_EXPORT]: SupportExportListResult;\n");
    output.push_str("  [DIAGNOSTICS_CREATE_SUPPORT_EXPORT]: SupportExportCreateResult;\n");
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
    output.push_str("  [MCP_GET_SERVER_DETAILS]: McpGetServerDetailsResult;\n");
    output.push_str("  [MCP_CREATE_SERVER]: McpMutationResult;\n");
    output.push_str("  [MCP_UPDATE_SERVER]: McpMutationResult;\n");
    output.push_str("  [MCP_DELETE_SERVER]: McpMutationResult;\n");
    output.push_str("  [MCP_SET_SERVER_ENABLED]: McpMutationResult;\n");
    output.push_str("  [SETTINGS_GET_SKILLS]: SettingsSkillsResult;\n");
    output.push_str("  [SETTINGS_GET_SKILL_DETAILS]: SettingsSkillDetailsResult;\n");
    output.push_str("  [SETTINGS_GET_PREFERENCES]: AppPreferencesResult;\n");
    output.push_str("  [SETTINGS_UPDATE_PREFERENCES]: AppPreferencesResult;\n");
    output.push_str("  [SETTINGS_UPDATE_NEW_TASK_DEFAULTS]: NewTaskDefaultsSnapshot;\n");
    output.push_str("  [SETTINGS_GET_RUNTIME]: RuntimeSettingsResult;\n");
    output.push_str("  [SETTINGS_UPDATE_RUNTIME]: RuntimeSettingsResult;\n");
    output.push_str("  [SETTINGS_RESET_TASK_HISTORY]: ResetTaskHistoryResult;\n");
    output.push_str("  [ATTACHMENT_LIST_ROOTS]: AttachmentListRootsResult;\n");
    output.push_str("  [ATTACHMENT_LIST_DIRECTORY]: AttachmentListDirectoryResult;\n");
    output.push_str("  [ATTACHMENT_CREATE_FILE_REFERENCE]: AttachmentCreateFileReferenceResult;\n");
    output.push_str(
        "  [ATTACHMENT_CREATE_LOCAL_FILE_REFERENCES]: AttachmentCreateLocalFileReferencesResult;\n",
    );
    output.push_str("  [ATTACHMENT_CREATE_PASTED_IMAGE]: AttachmentCreatePastedImageResult;\n");
    output.push_str(
        "  [ATTACHMENT_CREATE_EMBEDDED_CANDIDATE]: AttachmentCreateEmbeddedCandidateResult;\n",
    );
    output.push_str("  [ATTACHMENT_CONFIRM_EMBEDDED]: AttachmentConfirmEmbeddedResult;\n");
    output.push_str("  [ATTACHMENT_REFRESH_HANDLES]: AttachmentRefreshHandlesResult;\n");
    output.push_str("  [ATTACHMENT_RELEASE]: AttachmentReleaseResult;\n");
    output.push_str("  [ATTACHMENT_REVEAL]: AttachmentRevealResult;\n");
    output.push_str("  [ATTACHMENT_REVEAL_SENT]: AttachmentRevealSentResult;\n");
    output.push_str("  [SHELL_RESOLVE_FILE_REVEAL]: ShellResolveFileRevealResult;\n");
    output.push_str("  [WORKSPACE_LIST_ROOTS]: WorkspaceListRootsResult;\n");
    output.push_str("  [WORKSPACE_LIST_DIRECTORY]: WorkspaceListDirectoryResult;\n");
    output.push_str("  [PROJECT_ADD]: ProjectAddResult;\n");
    output.push_str("  [PROJECT_RENAME]: ProjectRenameResult;\n");
    output.push_str("  [PROJECT_REMOVE]: ProjectRemoveResult;\n");
    output.push_str("  [PROJECT_REFRESH]: ProjectRefreshResult;\n");
    output.push_str("  [WORKTREE_REFRESH]: WorktreeRefreshResult;\n");
    output.push_str("  [WORKTREE_CREATE]: WorktreeCreateResult;\n");
    output.push_str("  [WORKTREE_RECREATE]: WorktreeRecreateResult;\n");
    output.push_str("  [WORKTREE_REMOVAL_PREFLIGHT]: WorktreeRemovalPreflightResult;\n");
    output.push_str("  [WORKTREE_REMOVE]: WorktreeRemoveResult;\n");
    output.push_str("  [WORKTREE_RENAME]: WorktreeRenameResult;\n");
    output.push_str("  [WORKTREE_RESOLVE_FOLDER]: WorktreeResolveFolderResult;\n");
    output.push_str("  [WORKTREE_LINKED_TASKS]: WorktreeLinkedTasksResult;\n");
    output.push_str("  [TASK_ACQUIRE]: TaskAcquireResult;\n");
    output.push_str("  [TASK_ACQUIRE_IN_WORKTREE]: TaskAcquireInWorktreeResult;\n");
    output.push_str("  [TASK_SEARCH_FILES]: TaskSearchFilesResult;\n");
    output.push_str("  [TASK_ADOPT_NATIVE_SESSION]: TaskAdoptNativeSessionResult;\n");
    output.push_str("  [TASK_SEND]: TaskSendResult;\n");
    output.push_str("  [TASK_QUEUE_APPEND]: TaskQueueAppendResult;\n");
    output.push_str("  [TASK_QUEUE_REMOVE]: TaskQueueRemoveResult;\n");
    output.push_str("  [TASK_QUEUE_TAKE]: TaskQueueTakeResult;\n");
    output.push_str("  [TASK_QUEUE_MOVE]: TaskQueueMoveResult;\n");
    output.push_str("  [TASK_SET_CONFIG_OPTION]: TaskSetConfigOptionResult;\n");
    output.push_str("  [TASK_SET_PERMISSION_POLICY]: TaskSetPermissionPolicyResult;\n");
    output.push_str("  [TASK_SET_TITLE]: TaskSetTitleResult;\n");
    output.push_str("  [TASK_SET_PINNED]: TaskSetPinnedResult;\n");
    output.push_str("  [TASK_CLOSE_PLAN]: TaskClosePlanResult;\n");
    output.push_str("  [TASK_TOOL_IMAGE_PREVIEW]: TaskToolImagePreviewResult;\n");
    output.push_str("  [FILE_VIEWER_OPEN]: FileViewerSnapshot;\n");
    output.push_str("  [FILE_VIEWER_OPEN_FROM_HANDLE]: FileViewerSnapshot;\n");
    output.push_str("  [FILE_VIEWER_REFRESH]: FileViewerSnapshot;\n");
    output.push_str("  [FILE_VIEWER_RELEASE]: FileViewerReleaseResult;\n");
    output.push_str("  [TASK_CANCEL]: TaskCancelResult;\n");
    output.push_str("  [TASK_OPEN]: TaskOpenResult;\n");
    output.push_str("  [TASK_RELOAD_NATIVE_SESSION]: TaskReloadNativeSessionResult;\n");
    output.push_str("  [TASK_MARK_READ]: TaskMarkReadResult;\n");
    output.push_str("  [TASK_CHAT_PAGE]: TaskChatPageResult;\n");
    output.push_str("  [TASK_COMPOSER_HISTORY]: ComposerHistoryResult;\n");
    output.push_str("  [TASK_LIST]: TaskListResult;\n");
    output.push_str("  [TASK_NAVIGATION_REFRESH]: TaskNavigationRefreshResult;\n");
    output.push_str("  [TASK_NAVIGATION_LOAD_MORE]: TaskNavigationLoadMoreResult;\n");
    output.push_str("  [NATIVE_SESSION_ARCHIVE]: NativeSessionArchiveResult;\n");
    output.push_str("  [NATIVE_SESSION_SET_TITLE]: NativeSessionSetTitleResult;\n");
    output.push_str("  [NATIVE_SESSION_SET_PINNED]: NativeSessionSetPinnedResult;\n");
    output.push_str("  [NATIVE_SESSION_RESTORE]: NativeSessionRestoreResult;\n");
    output.push_str("  [NATIVE_SESSION_FORK]: NativeSessionForkResult;\n");
    output.push_str("  [TASK_RELEASE]: TaskReleaseResult;\n");
    output.push_str("  [TASK_ARCHIVE]: TaskArchiveResult;\n  [TASK_ARCHIVE_OLDER]: TaskArchiveOlderResult;\n  [TASK_RESTORE]: TaskRestoreResult;\n");
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
    output
        .push_str("export type ClientDetachRequest = TypedClientRequest<typeof CLIENT_DETACH>;\n");
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
    output.push_str("export type ClientDetachResponse = ResponseEnvelope<ClientDetachResult>;\n");
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
        "export type McpGetServerDetailsResponse = ResponseEnvelope<McpGetServerDetailsResult>;\n",
    );
    output.push_str("export type McpCreateServerResponse = ResponseEnvelope<McpMutationResult>;\n");
    output.push_str("export type McpUpdateServerResponse = ResponseEnvelope<McpMutationResult>;\n");
    output.push_str("export type McpDeleteServerResponse = ResponseEnvelope<McpMutationResult>;\n");
    output.push_str(
        "export type McpSetServerEnabledResponse = ResponseEnvelope<McpMutationResult>;\n",
    );
    output.push_str(
        "export type SettingsGetSkillsResponse = ResponseEnvelope<SettingsSkillsResult>;\n",
    );
    output.push_str(
        "export type SettingsGetSkillDetailsResponse = ResponseEnvelope<SettingsSkillDetailsResult>;\n",
    );
    output.push_str(
        "export type SettingsGetPreferencesResponse = ResponseEnvelope<AppPreferencesResult>;\n",
    );
    output.push_str(
        "export type SettingsUpdatePreferencesResponse = ResponseEnvelope<AppPreferencesResult>;\n",
    );
    output.push_str(
        "export type SettingsUpdateNewTaskDefaultsResponse = ResponseEnvelope<NewTaskDefaultsSnapshot>;\n",
    );
    output.push_str(
        "export type SettingsGetRuntimeResponse = ResponseEnvelope<RuntimeSettingsResult>;\n",
    );
    output.push_str(
        "export type SettingsUpdateRuntimeResponse = ResponseEnvelope<RuntimeSettingsResult>;\n",
    );
    output.push_str(
        "export type SettingsResetTaskHistoryResponse = ResponseEnvelope<ResetTaskHistoryResult>;\n",
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
        "export type AttachmentCreateLocalFileReferencesResponse = ResponseEnvelope<AttachmentCreateLocalFileReferencesResult>;\n",
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
        "export type AttachmentRevealSentResponse = ResponseEnvelope<AttachmentRevealSentResult>;\n",
    );
    output.push_str(
        "export type WorkspaceListRootsResponse = ResponseEnvelope<WorkspaceListRootsResult>;\n",
    );
    output.push_str("export type ProjectAddRequest = TypedClientRequest<typeof PROJECT_ADD>;\n");
    output.push_str("export type ProjectAddResponse = ResponseEnvelope<ProjectAddResult>;\n");
    output.push_str(
        "export type ProjectRenameRequest = TypedClientRequest<typeof PROJECT_RENAME>;\n",
    );
    output.push_str("export type ProjectRenameResponse = ResponseEnvelope<ProjectRenameResult>;\n");
    output.push_str(
        "export type ProjectRemoveRequest = TypedClientRequest<typeof PROJECT_REMOVE>;\n",
    );
    output.push_str("export type ProjectRemoveResponse = ResponseEnvelope<ProjectRemoveResult>;\n");
    output.push_str(
        "export type ProjectRefreshRequest = TypedClientRequest<typeof PROJECT_REFRESH>;\n",
    );
    output
        .push_str("export type ProjectRefreshResponse = ResponseEnvelope<ProjectRefreshResult>;\n");
    output.push_str(
        "export type WorkspaceListDirectoryResponse = ResponseEnvelope<WorkspaceListDirectoryResult>;\n",
    );
    output.push_str("export type TaskAcquireResponse = ResponseEnvelope<TaskAcquireResult>;\n");
    output.push_str(
        "export type TaskSearchFilesResponse = ResponseEnvelope<TaskSearchFilesResult>;\n",
    );
    output.push_str(
        "export type TaskAdoptNativeSessionResponse = ResponseEnvelope<TaskAdoptNativeSessionResult>;\n",
    );
    output.push_str("export type TaskSendResponse = ResponseEnvelope<TaskSendResult>;\n");
    output.push_str(
        "export type TaskQueueAppendResponse = ResponseEnvelope<TaskQueueAppendResult>;\n",
    );
    output.push_str(
        "export type TaskQueueRemoveResponse = ResponseEnvelope<TaskQueueRemoveResult>;\n",
    );
    output.push_str("export type TaskQueueTakeResponse = ResponseEnvelope<TaskQueueTakeResult>;\n");
    output.push_str("export type TaskQueueMoveResponse = ResponseEnvelope<TaskQueueMoveResult>;\n");
    output.push_str(
        "export type TaskSetConfigOptionResponse = ResponseEnvelope<TaskSetConfigOptionResult>;\n",
    );
    output.push_str(
        "export type TaskSetPermissionPolicyResponse = ResponseEnvelope<TaskSetPermissionPolicyResult>;\n",
    );
    output.push_str("export type TaskCancelResponse = ResponseEnvelope<TaskCancelResult>;\n");
    output.push_str("export type TaskOpenResponse = ResponseEnvelope<TaskOpenResult>;\n");
    output.push_str(
        "export type TaskReloadNativeSessionResponse = ResponseEnvelope<TaskReloadNativeSessionResult>;\n",
    );
    output.push_str("export type TaskChatPageResponse = ResponseEnvelope<TaskChatPageResult>;\n");
    output.push_str(
        "export type TaskComposerHistoryResponse = ResponseEnvelope<ComposerHistoryResult>;\n",
    );
    output.push_str("export type TaskListResponse = ResponseEnvelope<TaskListResult>;\n");
    output.push_str("export type TaskReleaseResponse = ResponseEnvelope<TaskReleaseResult>;\n");
    output.push_str(
        "export type TaskArchiveResponse = ResponseEnvelope<TaskArchiveResult>;\nexport type TaskRestoreResponse = ResponseEnvelope<TaskRestoreResult>;\n",
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
