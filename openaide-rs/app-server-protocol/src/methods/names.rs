pub const CLIENT_PROBE: &str = "client/probe";
pub const CLIENT_INITIALIZE: &str = "client/initialize";
pub const CLIENT_CAPABILITIES_CHANGED: &str = "client/capabilitiesChanged";
pub const CLIENT_HEARTBEAT: &str = "client/heartbeat";
pub const CLIENT_DETACH: &str = "client/detach";
pub const PENDING_REQUEST_RESOLVE: &str = "pendingRequest/resolve";
pub const STATE_SUBSCRIBE: &str = "state/subscribe";
pub const STATE_UNSUBSCRIBE: &str = "state/unsubscribe";
pub const DIAGNOSTICS_GET_RUNTIME: &str = "diagnostics/getRuntime";
pub const SUPPORT_RECOVER_STUCK_SESSIONS: &str = "support/recoverStuckSessions";
pub const AGENT_PROBE: &str = "agent/probe";
pub const AGENT_AUTHENTICATE: &str = "agent/authenticate";
pub const AGENT_LIST_SESSIONS: &str = "agent/listSessions";
pub const AGENT_CREATE_CUSTOM: &str = "agent/createCustom";
pub const AGENT_UPDATE_CUSTOM_METADATA: &str = "agent/updateCustomMetadata";
pub const AGENT_REPLACE_CUSTOM: &str = "agent/replaceCustom";
pub const AGENT_DELETE_CUSTOM: &str = "agent/deleteCustom";
pub const AGENT_SET_ENABLED: &str = "agent/setEnabled";
pub const SETTINGS_GET_AGENT_DETAILS: &str = "settings/getAgentDetails";
pub const SETTINGS_GET_MCP_SERVERS: &str = "settings/getMcpServers";
pub const SETTINGS_GET_SKILLS: &str = "settings/getSkills";
pub const SETTINGS_GET_PREFERENCES: &str = "settings/getPreferences";
pub const SETTINGS_UPDATE_PREFERENCES: &str = "settings/updatePreferences";
pub const SETTINGS_GET_RUNTIME: &str = "settings/getRuntime";
pub const SETTINGS_UPDATE_RUNTIME: &str = "settings/updateRuntime";
pub const ATTACHMENT_LIST_ROOTS: &str = "attachment/listRoots";
pub const ATTACHMENT_LIST_DIRECTORY: &str = "attachment/listDirectory";
pub const ATTACHMENT_CREATE_FILE_REFERENCE: &str = "attachment/createFileReference";
pub const ATTACHMENT_CREATE_LOCAL_FILE_REFERENCES: &str = "attachment/createLocalFileReferences";
pub const ATTACHMENT_CREATE_PASTED_IMAGE: &str = "attachment/createPastedImage";
pub const ATTACHMENT_CREATE_EMBEDDED_CANDIDATE: &str = "attachment/createEmbeddedCandidate";
pub const ATTACHMENT_CONFIRM_EMBEDDED: &str = "attachment/confirmEmbedded";
pub const ATTACHMENT_REFRESH_HANDLES: &str = "attachment/refreshHandles";
pub const ATTACHMENT_RELEASE: &str = "attachment/release";
pub const ATTACHMENT_REVEAL: &str = "attachment/reveal";
pub const ATTACHMENT_REVEAL_SENT: &str = "attachment/revealSent";
pub const SHELL_RESOLVE_FILE_REVEAL: &str = "shell/resolveFileReveal";
pub const WORKSPACE_LIST_ROOTS: &str = "workspace/listRoots";
pub const WORKSPACE_LIST_DIRECTORY: &str = "workspace/listDirectory";
pub const WORKTREE_REFRESH: &str = "worktree/refresh";
pub const WORKTREE_CREATE: &str = "worktree/create";
pub const WORKTREE_RECREATE: &str = "worktree/recreate";
pub const WORKTREE_REMOVAL_PREFLIGHT: &str = "worktree/removalPreflight";
pub const WORKTREE_REMOVE: &str = "worktree/remove";
pub const WORKTREE_RENAME: &str = "worktree/rename";
pub const WORKTREE_RESOLVE_FOLDER: &str = "worktree/resolveFolder";
pub const WORKTREE_LINKED_TASKS: &str = "worktree/linkedTasks";
pub const TASK_ACQUIRE_IN_WORKTREE: &str = "task/acquireInWorktree";
pub const TASK_ACQUIRE: &str = "task/acquire";
pub const TASK_SEARCH_FILES: &str = "task/searchFiles";
pub const TASK_ADOPT_NATIVE_SESSION: &str = "task/adoptNativeSession";
pub const TASK_SEND: &str = "task/send";
pub const TASK_SET_CONFIG_OPTION: &str = "task/setConfigOption";
pub const TASK_SET_TITLE: &str = "task/setTitle";
pub const TASK_CANCEL: &str = "task/cancel";
pub const TASK_OPEN: &str = "task/open";
pub const TASK_MARK_READ: &str = "task/markRead";
pub const TASK_CHAT_PAGE: &str = "task/chatPage";
pub const TASK_LIST: &str = "task/list";
pub const TASK_NAVIGATION_REFRESH: &str = "taskNavigation/refresh";
pub const TASK_NAVIGATION_LOAD_MORE: &str = "taskNavigation/loadMore";
pub const NATIVE_SESSION_ARCHIVE: &str = "nativeSession/archive";
pub const NATIVE_SESSION_RESTORE: &str = "nativeSession/restore";
pub const TASK_RELEASE: &str = "task/release";
pub const TASK_ARCHIVE: &str = "task/archive";
pub const TASK_RESTORE: &str = "task/restore";

pub const CLIENT_METHODS: &[&str] = &[
    CLIENT_PROBE,
    CLIENT_INITIALIZE,
    CLIENT_CAPABILITIES_CHANGED,
    CLIENT_HEARTBEAT,
    CLIENT_DETACH,
    PENDING_REQUEST_RESOLVE,
    STATE_SUBSCRIBE,
    STATE_UNSUBSCRIBE,
    DIAGNOSTICS_GET_RUNTIME,
    SUPPORT_RECOVER_STUCK_SESSIONS,
    AGENT_PROBE,
    AGENT_AUTHENTICATE,
    AGENT_LIST_SESSIONS,
    AGENT_CREATE_CUSTOM,
    AGENT_UPDATE_CUSTOM_METADATA,
    AGENT_REPLACE_CUSTOM,
    AGENT_DELETE_CUSTOM,
    AGENT_SET_ENABLED,
    SETTINGS_GET_AGENT_DETAILS,
    SETTINGS_GET_MCP_SERVERS,
    SETTINGS_GET_SKILLS,
    SETTINGS_GET_PREFERENCES,
    SETTINGS_UPDATE_PREFERENCES,
    SETTINGS_GET_RUNTIME,
    SETTINGS_UPDATE_RUNTIME,
    ATTACHMENT_LIST_ROOTS,
    ATTACHMENT_LIST_DIRECTORY,
    ATTACHMENT_CREATE_FILE_REFERENCE,
    ATTACHMENT_CREATE_LOCAL_FILE_REFERENCES,
    ATTACHMENT_CREATE_PASTED_IMAGE,
    ATTACHMENT_CREATE_EMBEDDED_CANDIDATE,
    ATTACHMENT_CONFIRM_EMBEDDED,
    ATTACHMENT_REFRESH_HANDLES,
    ATTACHMENT_RELEASE,
    ATTACHMENT_REVEAL,
    ATTACHMENT_REVEAL_SENT,
    SHELL_RESOLVE_FILE_REVEAL,
    WORKSPACE_LIST_ROOTS,
    WORKSPACE_LIST_DIRECTORY,
    WORKTREE_REFRESH,
    WORKTREE_CREATE,
    WORKTREE_RECREATE,
    WORKTREE_REMOVAL_PREFLIGHT,
    WORKTREE_REMOVE,
    WORKTREE_RENAME,
    WORKTREE_RESOLVE_FOLDER,
    WORKTREE_LINKED_TASKS,
    TASK_ACQUIRE_IN_WORKTREE,
    TASK_ACQUIRE,
    TASK_SEARCH_FILES,
    TASK_ADOPT_NATIVE_SESSION,
    TASK_SEND,
    TASK_SET_CONFIG_OPTION,
    TASK_SET_TITLE,
    TASK_CANCEL,
    TASK_OPEN,
    TASK_MARK_READ,
    TASK_CHAT_PAGE,
    TASK_LIST,
    TASK_NAVIGATION_REFRESH,
    TASK_NAVIGATION_LOAD_MORE,
    NATIVE_SESSION_ARCHIVE,
    NATIVE_SESSION_RESTORE,
    TASK_RELEASE,
    TASK_ARCHIVE,
    TASK_RESTORE,
];
