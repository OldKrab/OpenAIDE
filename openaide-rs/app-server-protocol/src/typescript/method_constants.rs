use crate::methods::{
    AGENT_AUTHENTICATE, AGENT_CREATE_CUSTOM, AGENT_DELETE_CUSTOM, AGENT_LIST_SESSIONS, AGENT_PROBE,
    AGENT_REPLACE_CUSTOM, AGENT_SET_ENABLED, AGENT_UPDATE_CUSTOM_METADATA,
    ATTACHMENT_CONFIRM_EMBEDDED, ATTACHMENT_CREATE_EMBEDDED_CANDIDATE,
    ATTACHMENT_CREATE_FILE_REFERENCE, ATTACHMENT_CREATE_LOCAL_FILE_REFERENCES,
    ATTACHMENT_CREATE_PASTED_IMAGE, ATTACHMENT_LIST_DIRECTORY, ATTACHMENT_LIST_ROOTS,
    ATTACHMENT_REFRESH_HANDLES, ATTACHMENT_RELEASE, ATTACHMENT_REVEAL, ATTACHMENT_REVEAL_SENT,
    CLIENT_CAPABILITIES_CHANGED, CLIENT_DETACH, CLIENT_HEARTBEAT, CLIENT_INITIALIZE, CLIENT_PROBE,
    DIAGNOSTICS_GET_RUNTIME, PENDING_REQUEST_RESOLVE, SETTINGS_GET_AGENT_DETAILS,
    SETTINGS_GET_MCP_SERVERS, SETTINGS_GET_PREFERENCES, SETTINGS_GET_RUNTIME, SETTINGS_GET_SKILLS,
    SETTINGS_UPDATE_PREFERENCES, SETTINGS_UPDATE_RUNTIME, SHELL_RESOLVE_FILE_REVEAL,
    STATE_SUBSCRIBE, STATE_UNSUBSCRIBE, SUPPORT_RECOVER_STUCK_SESSIONS, TASK_ACQUIRE,
    TASK_ACQUIRE_IN_WORKTREE, TASK_ADOPT_NATIVE_SESSION, TASK_ARCHIVE, TASK_CANCEL, TASK_CHAT_PAGE,
    TASK_LIST, TASK_MARK_READ, TASK_NAVIGATION_LOAD_MORE, TASK_NAVIGATION_REFRESH, TASK_OPEN,
    TASK_RELEASE, TASK_RESTORE, TASK_SEARCH_FILES, TASK_SEND, TASK_SET_CONFIG_OPTION,
    WORKSPACE_LIST_DIRECTORY, WORKSPACE_LIST_ROOTS, WORKTREE_CREATE, WORKTREE_LINKED_TASKS,
    WORKTREE_RECREATE, WORKTREE_REFRESH, WORKTREE_REMOVAL_PREFLIGHT, WORKTREE_REMOVE,
    WORKTREE_RENAME, WORKTREE_RESOLVE_FOLDER,
};
use crate::server_requests::{
    PERMISSION_REQUEST, QUESTION_REQUEST, SECRET_READ, SHELL_REVEAL_FILE, SHELL_SHOW_NOTIFICATION,
};

pub(super) fn push_method_constants(output: &mut String) {
    output.push_str(&format!(
        "export const CLIENT_PROBE = {:?} as const;\n",
        CLIENT_PROBE
    ));
    output.push_str(&format!(
        "export const CLIENT_INITIALIZE = {:?} as const;\n",
        CLIENT_INITIALIZE
    ));
    output.push_str(&format!(
        "export const CLIENT_CAPABILITIES_CHANGED = {:?} as const;\n",
        CLIENT_CAPABILITIES_CHANGED
    ));
    output.push_str(&format!(
        "export const CLIENT_HEARTBEAT = {:?} as const;\n",
        CLIENT_HEARTBEAT
    ));
    output.push_str(&format!(
        "export const CLIENT_DETACH = {:?} as const;\n\n",
        CLIENT_DETACH
    ));
    output.push_str(&format!(
        "export const PENDING_REQUEST_RESOLVE = {:?} as const;\n\n",
        PENDING_REQUEST_RESOLVE
    ));
    output.push_str(&format!(
        "export const STATE_SUBSCRIBE = {:?} as const;\n",
        STATE_SUBSCRIBE
    ));
    output.push_str(&format!(
        "export const STATE_UNSUBSCRIBE = {:?} as const;\n\n",
        STATE_UNSUBSCRIBE
    ));
    output.push_str(&format!(
        "export const DIAGNOSTICS_GET_RUNTIME = {:?} as const;\n\n",
        DIAGNOSTICS_GET_RUNTIME
    ));
    output.push_str(&format!(
        "export const SUPPORT_RECOVER_STUCK_SESSIONS = {:?} as const;\n\n",
        SUPPORT_RECOVER_STUCK_SESSIONS
    ));
    output.push_str(&format!(
        "export const AGENT_PROBE = {:?} as const;\n\n",
        AGENT_PROBE
    ));
    output.push_str(&format!(
        "export const AGENT_AUTHENTICATE = {:?} as const;\n",
        AGENT_AUTHENTICATE
    ));
    output.push_str(&format!(
        "export const AGENT_LIST_SESSIONS = {:?} as const;\n",
        AGENT_LIST_SESSIONS
    ));
    output.push_str(&format!(
        "export const AGENT_CREATE_CUSTOM = {:?} as const;\n",
        AGENT_CREATE_CUSTOM
    ));
    output.push_str(&format!(
        "export const AGENT_UPDATE_CUSTOM_METADATA = {:?} as const;\n",
        AGENT_UPDATE_CUSTOM_METADATA
    ));
    output.push_str(&format!(
        "export const AGENT_REPLACE_CUSTOM = {:?} as const;\n",
        AGENT_REPLACE_CUSTOM
    ));
    output.push_str(&format!(
        "export const AGENT_DELETE_CUSTOM = {:?} as const;\n",
        AGENT_DELETE_CUSTOM
    ));
    output.push_str(&format!(
        "export const AGENT_SET_ENABLED = {:?} as const;\n\n",
        AGENT_SET_ENABLED
    ));
    output.push_str(&format!(
        "export const SETTINGS_GET_AGENT_DETAILS = {:?} as const;\n",
        SETTINGS_GET_AGENT_DETAILS
    ));
    output.push_str(&format!(
        "export const SETTINGS_GET_MCP_SERVERS = {:?} as const;\n",
        SETTINGS_GET_MCP_SERVERS
    ));
    output.push_str(&format!(
        "export const SETTINGS_GET_SKILLS = {:?} as const;\n",
        SETTINGS_GET_SKILLS
    ));
    output.push_str(&format!(
        "export const SETTINGS_GET_PREFERENCES = {:?} as const;\n",
        SETTINGS_GET_PREFERENCES
    ));
    output.push_str(&format!(
        "export const SETTINGS_UPDATE_PREFERENCES = {:?} as const;\n",
        SETTINGS_UPDATE_PREFERENCES
    ));
    output.push_str(&format!(
        "export const SETTINGS_GET_RUNTIME = {:?} as const;\n",
        SETTINGS_GET_RUNTIME
    ));
    output.push_str(&format!(
        "export const SETTINGS_UPDATE_RUNTIME = {:?} as const;\n\n",
        SETTINGS_UPDATE_RUNTIME
    ));
    output.push_str(&format!(
        "export const ATTACHMENT_LIST_ROOTS = {:?} as const;\n",
        ATTACHMENT_LIST_ROOTS
    ));
    output.push_str(&format!(
        "export const ATTACHMENT_LIST_DIRECTORY = {:?} as const;\n",
        ATTACHMENT_LIST_DIRECTORY
    ));
    output.push_str(&format!(
        "export const ATTACHMENT_CREATE_FILE_REFERENCE = {:?} as const;\n",
        ATTACHMENT_CREATE_FILE_REFERENCE
    ));
    output.push_str(&format!(
        "export const ATTACHMENT_CREATE_LOCAL_FILE_REFERENCES = {:?} as const;\n",
        ATTACHMENT_CREATE_LOCAL_FILE_REFERENCES
    ));
    output.push_str(&format!(
        "export const ATTACHMENT_CREATE_PASTED_IMAGE = {:?} as const;\n\n",
        ATTACHMENT_CREATE_PASTED_IMAGE
    ));
    output.push_str(&format!(
        "export const ATTACHMENT_CREATE_EMBEDDED_CANDIDATE = {:?} as const;\n",
        ATTACHMENT_CREATE_EMBEDDED_CANDIDATE
    ));
    output.push_str(&format!(
        "export const ATTACHMENT_CONFIRM_EMBEDDED = {:?} as const;\n\n",
        ATTACHMENT_CONFIRM_EMBEDDED
    ));
    output.push_str(&format!(
        "export const ATTACHMENT_REFRESH_HANDLES = {:?} as const;\n",
        ATTACHMENT_REFRESH_HANDLES
    ));
    output.push_str(&format!(
        "export const ATTACHMENT_RELEASE = {:?} as const;\n\n",
        ATTACHMENT_RELEASE
    ));
    output.push_str(&format!(
        "export const ATTACHMENT_REVEAL = {:?} as const;\n\n",
        ATTACHMENT_REVEAL
    ));
    output.push_str(&format!(
        "export const ATTACHMENT_REVEAL_SENT = {:?} as const;\n\n",
        ATTACHMENT_REVEAL_SENT
    ));
    output.push_str(&format!(
        "export const SHELL_RESOLVE_FILE_REVEAL = {:?} as const;\n\n",
        SHELL_RESOLVE_FILE_REVEAL
    ));
    output.push_str(&format!(
        "export const WORKSPACE_LIST_ROOTS = {:?} as const;\n",
        WORKSPACE_LIST_ROOTS
    ));
    output.push_str(&format!(
        "export const WORKSPACE_LIST_DIRECTORY = {:?} as const;\n\n",
        WORKSPACE_LIST_DIRECTORY
    ));
    output.push_str(&format!(
        "export const WORKTREE_REFRESH = {:?} as const;\n",
        WORKTREE_REFRESH
    ));
    output.push_str(&format!(
        "export const WORKTREE_CREATE = {:?} as const;\n",
        WORKTREE_CREATE
    ));
    output.push_str(&format!(
        "export const WORKTREE_RECREATE = {:?} as const;\n",
        WORKTREE_RECREATE
    ));
    output.push_str(&format!(
        "export const WORKTREE_REMOVAL_PREFLIGHT = {:?} as const;\n",
        WORKTREE_REMOVAL_PREFLIGHT
    ));
    output.push_str(&format!(
        "export const WORKTREE_REMOVE = {:?} as const;\n",
        WORKTREE_REMOVE
    ));
    output.push_str(&format!(
        "export const WORKTREE_RENAME = {:?} as const;\n",
        WORKTREE_RENAME
    ));
    output.push_str(&format!(
        "export const WORKTREE_RESOLVE_FOLDER = {:?} as const;\n",
        WORKTREE_RESOLVE_FOLDER
    ));
    output.push_str(&format!(
        "export const WORKTREE_LINKED_TASKS = {:?} as const;\n\n",
        WORKTREE_LINKED_TASKS
    ));
    output.push_str(&format!(
        "export const TASK_ACQUIRE = {:?} as const;\n",
        TASK_ACQUIRE
    ));
    output.push_str(&format!(
        "export const TASK_ACQUIRE_IN_WORKTREE = {:?} as const;\n",
        TASK_ACQUIRE_IN_WORKTREE
    ));
    output.push_str(&format!(
        "export const TASK_SEARCH_FILES = {:?} as const;\n",
        TASK_SEARCH_FILES
    ));
    output.push_str(&format!(
        "export const TASK_ADOPT_NATIVE_SESSION = {:?} as const;\n",
        TASK_ADOPT_NATIVE_SESSION
    ));
    output.push_str(&format!(
        "export const TASK_SEND = {:?} as const;\n",
        TASK_SEND
    ));
    output.push_str(&format!(
        "export const TASK_SET_CONFIG_OPTION = {:?} as const;\n",
        TASK_SET_CONFIG_OPTION
    ));
    output.push_str(&format!(
        "export const TASK_CANCEL = {:?} as const;\n",
        TASK_CANCEL
    ));
    output.push_str(&format!(
        "export const TASK_OPEN = {:?} as const;\n",
        TASK_OPEN
    ));
    output.push_str(&format!(
        "export const TASK_MARK_READ = {:?} as const;\n",
        TASK_MARK_READ
    ));
    output.push_str(&format!(
        "export const TASK_CHAT_PAGE = {:?} as const;\n",
        TASK_CHAT_PAGE
    ));
    output.push_str(&format!(
        "export const TASK_LIST = {:?} as const;\n",
        TASK_LIST
    ));
    output.push_str(&format!(
        "export const TASK_NAVIGATION_REFRESH = {:?} as const;\n",
        TASK_NAVIGATION_REFRESH
    ));
    output.push_str(&format!(
        "export const TASK_NAVIGATION_LOAD_MORE = {:?} as const;\n",
        TASK_NAVIGATION_LOAD_MORE
    ));
    output.push_str(&format!(
        "export const TASK_RELEASE = {:?} as const;\n\n",
        TASK_RELEASE
    ));
    output.push_str(&format!(
        "export const TASK_ARCHIVE = {:?} as const;\n\nexport const TASK_RESTORE = {:?} as const;\n\n",
        TASK_ARCHIVE,
        TASK_RESTORE
    ));
    output.push_str(&format!(
        "export const PERMISSION_REQUEST = {:?} as const;\n\n",
        PERMISSION_REQUEST
    ));
    output.push_str(&format!(
        "export const QUESTION_REQUEST = {:?} as const;\n\n",
        QUESTION_REQUEST
    ));
    output.push_str(&format!(
        "export const SECRET_READ = {:?} as const;\n",
        SECRET_READ
    ));
    output.push_str(&format!(
        "export const SHELL_SHOW_NOTIFICATION = {:?} as const;\n",
        SHELL_SHOW_NOTIFICATION
    ));
    output.push_str(&format!(
        "export const SHELL_REVEAL_FILE = {:?} as const;\n\n",
        SHELL_REVEAL_FILE
    ));
}
