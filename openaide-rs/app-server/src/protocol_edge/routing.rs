use openaide_app_server_protocol::methods::{
    AGENT_AUTHENTICATE, AGENT_CREATE_CUSTOM, AGENT_DELETE_CUSTOM, AGENT_LIST_SESSIONS, AGENT_PROBE,
    AGENT_REPLACE_CUSTOM, AGENT_SET_ENABLED, AGENT_UPDATE_CUSTOM_METADATA,
    ATTACHMENT_CONFIRM_EMBEDDED, ATTACHMENT_CREATE_EMBEDDED_CANDIDATE,
    ATTACHMENT_CREATE_FILE_REFERENCE, ATTACHMENT_CREATE_PASTED_IMAGE, ATTACHMENT_LIST_DIRECTORY,
    ATTACHMENT_LIST_ROOTS, ATTACHMENT_REFRESH_HANDLES, ATTACHMENT_RELEASE, ATTACHMENT_REVEAL,
    CLIENT_CAPABILITIES_CHANGED, CLIENT_HEARTBEAT, CLIENT_INITIALIZE, CLIENT_PROBE,
    DIAGNOSTICS_GET_RUNTIME, SETTINGS_GET_AGENT_DETAILS, SETTINGS_GET_MCP_SERVERS,
    SETTINGS_GET_PREFERENCES, SETTINGS_GET_RUNTIME, SETTINGS_GET_SKILLS,
    SETTINGS_UPDATE_PREFERENCES, SETTINGS_UPDATE_RUNTIME, SHELL_RESOLVE_FILE_REVEAL,
    STATE_SUBSCRIBE, STATE_UNSUBSCRIBE, SUPPORT_RECOVER_STUCK_SESSIONS, TASK_ADOPT_NATIVE_SESSION,
    TASK_CANCEL, TASK_CHAT_PAGE, TASK_CREATE, TASK_DISCARD, TASK_LIST, TASK_MARK_READ, TASK_OPEN,
    TASK_SEND, TASK_SET_ARCHIVED, TASK_SET_CONFIG_OPTION, TASK_TOOL_DETAIL,
    WORKSPACE_LIST_DIRECTORY, WORKSPACE_LIST_ROOTS,
};

use crate::client_lifecycle::{AppServerTime, ConnectionId};
use crate::protocol_edge::{responses, GatewayOutcome, InboundProtocolMessage, RpcGateway};

impl RpcGateway {
    pub fn handle_inbound(
        &mut self,
        connection_id: ConnectionId,
        message: InboundProtocolMessage,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let (id, method, params, meta) = match message {
            InboundProtocolMessage::ClientRequest {
                id,
                method,
                params,
                meta,
            } => (id, method, params, meta),
            InboundProtocolMessage::ClientResponse { request_id, answer } => {
                return self.handle_client_response(connection_id, request_id, answer, now);
            }
            InboundProtocolMessage::ClientNotification { .. } => return GatewayOutcome::Noop,
        };

        if method != CLIENT_INITIALIZE
            && method != CLIENT_PROBE
            && self
                .client_hub
                .context_for_connection(&connection_id)
                .is_none()
        {
            return self.error(connection_id, id, meta, responses::not_initialized(method));
        }
        if method != CLIENT_INITIALIZE && method != CLIENT_PROBE {
            if let Some(client_instance_id) = self
                .client_hub
                .observe_connection_activity(&connection_id, now)
            {
                self.attachments.keep_alive_for_client(&client_instance_id);
            }
        }

        match method.as_str() {
            CLIENT_PROBE => self.handle_client_probe(connection_id, id, params, meta),
            CLIENT_INITIALIZE => self.handle_initialize(connection_id, id, params, meta, now),
            CLIENT_CAPABILITIES_CHANGED => {
                self.handle_client_capabilities_changed(connection_id, id, params, meta, now)
            }
            CLIENT_HEARTBEAT => self.handle_client_heartbeat(connection_id, id, params, meta, now),
            STATE_SUBSCRIBE => self.handle_subscribe(connection_id, id, params, meta, now),
            STATE_UNSUBSCRIBE => self.handle_unsubscribe(connection_id, id, params, meta, now),
            DIAGNOSTICS_GET_RUNTIME => {
                self.handle_diagnostics_get_runtime(connection_id, id, params, meta)
            }
            SUPPORT_RECOVER_STUCK_SESSIONS => {
                self.handle_support_recover_stuck_sessions(connection_id, id, params, meta, now)
            }
            AGENT_PROBE => self.handle_agent_probe(connection_id, id, params, meta, now),
            AGENT_AUTHENTICATE => self.handle_agent_authenticate(connection_id, id, params, meta),
            AGENT_LIST_SESSIONS => self.handle_agent_list_sessions(connection_id, id, params, meta),
            AGENT_CREATE_CUSTOM => {
                self.handle_agent_create_custom(connection_id, id, params, meta, now)
            }
            AGENT_UPDATE_CUSTOM_METADATA => {
                self.handle_agent_update_custom_metadata(connection_id, id, params, meta, now)
            }
            AGENT_REPLACE_CUSTOM => {
                self.handle_agent_replace_custom(connection_id, id, params, meta, now)
            }
            AGENT_DELETE_CUSTOM => {
                self.handle_agent_delete_custom(connection_id, id, params, meta, now)
            }
            AGENT_SET_ENABLED => {
                self.handle_agent_set_enabled(connection_id, id, params, meta, now)
            }
            SETTINGS_GET_AGENT_DETAILS => {
                self.handle_settings_get_agent_details(connection_id, id, params, meta)
            }
            SETTINGS_GET_MCP_SERVERS => {
                self.handle_settings_get_mcp_servers(connection_id, id, params, meta)
            }
            SETTINGS_GET_SKILLS => self.handle_settings_get_skills(connection_id, id, params, meta),
            SETTINGS_GET_PREFERENCES => {
                self.handle_settings_get_preferences(connection_id, id, params, meta)
            }
            SETTINGS_UPDATE_PREFERENCES => {
                self.handle_settings_update_preferences(connection_id, id, params, meta)
            }
            SETTINGS_GET_RUNTIME => {
                self.handle_settings_get_runtime(connection_id, id, params, meta)
            }
            SETTINGS_UPDATE_RUNTIME => {
                self.handle_settings_update_runtime(connection_id, id, params, meta)
            }
            ATTACHMENT_LIST_ROOTS => {
                self.handle_attachment_list_roots(connection_id, id, params, meta)
            }
            ATTACHMENT_LIST_DIRECTORY => {
                self.handle_attachment_list_directory(connection_id, id, params, meta)
            }
            ATTACHMENT_CREATE_FILE_REFERENCE => {
                self.handle_attachment_create_file_reference(connection_id, id, params, meta)
            }
            ATTACHMENT_CREATE_PASTED_IMAGE => {
                self.handle_attachment_create_pasted_image(connection_id, id, params, meta)
            }
            ATTACHMENT_CREATE_EMBEDDED_CANDIDATE => {
                self.handle_attachment_create_embedded_candidate(connection_id, id, params, meta)
            }
            ATTACHMENT_CONFIRM_EMBEDDED => {
                self.handle_attachment_confirm_embedded(connection_id, id, params, meta)
            }
            ATTACHMENT_REFRESH_HANDLES => {
                self.handle_attachment_refresh_handles(connection_id, id, params, meta)
            }
            ATTACHMENT_RELEASE => self.handle_attachment_release(connection_id, id, params, meta),
            ATTACHMENT_REVEAL => {
                self.handle_attachment_reveal(connection_id, id, params, meta, now)
            }
            SHELL_RESOLVE_FILE_REVEAL => {
                self.handle_shell_resolve_file_reveal(connection_id, id, params, meta)
            }
            WORKSPACE_LIST_ROOTS => {
                self.handle_workspace_list_roots(connection_id, id, params, meta)
            }
            WORKSPACE_LIST_DIRECTORY => {
                self.handle_workspace_list_directory(connection_id, id, params, meta)
            }
            TASK_CREATE => self.handle_task_create(connection_id, id, params, meta, now),
            TASK_ADOPT_NATIVE_SESSION => {
                self.handle_task_adopt_native_session(connection_id, id, params, meta, now)
            }
            TASK_SEND => self.handle_task_send(connection_id, id, params, meta, now),
            TASK_CANCEL => self.handle_task_cancel(connection_id, id, params, meta, now),
            TASK_CHAT_PAGE => self.handle_task_chat_page(connection_id, id, params, meta),
            TASK_TOOL_DETAIL => self.handle_task_tool_detail(connection_id, id, params, meta),
            TASK_SET_CONFIG_OPTION => {
                self.handle_task_set_config_option(connection_id, id, params, meta, now)
            }
            TASK_DISCARD => self.handle_task_discard(connection_id, id, params, meta, now),
            TASK_SET_ARCHIVED => {
                self.handle_task_set_archived(connection_id, id, params, meta, now)
            }
            TASK_LIST => self.handle_task_list(connection_id, id, params, meta),
            TASK_OPEN => self.handle_task_open(connection_id, id, params, meta),
            TASK_MARK_READ => self.handle_task_mark_read(connection_id, id, params, meta, now),
            _ => self.error(
                connection_id,
                id,
                meta,
                responses::unsupported_method(&method),
            ),
        }
    }
}
