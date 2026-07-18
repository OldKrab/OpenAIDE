use std::sync::Arc;

use openaide_app_server_protocol::client::{
    ClientProbeLifecycle, ClientProbeParams, ClientProbeResult, InitializeParams, InitializeResult,
    APP_SERVER_PROTOCOL_VERSION,
};
mod agent_handlers;
mod attachment_handlers;
mod client_handlers;
mod diagnostics_handlers;
pub mod local_http;
mod messages;
mod responses;
mod routing;
mod server_request_handlers;
mod settings_handlers;
mod shared_gateway;
mod shell_handlers;
pub mod stdio;
mod support_handlers;
mod task_handlers;
mod worktree_handlers;

pub(crate) use messages::event_deliveries;
pub use messages::{GatewayEventDelivery, GatewayOutcome, GatewayResponse, InboundProtocolMessage};
pub use shared_gateway::SharedRpcGateway;

use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::methods::{STATE_SUBSCRIBE, STATE_UNSUBSCRIBE};
use openaide_app_server_protocol::state::{
    StateSubscribeParams, StateSubscribeResult, StateUnsubscribeParams, StateUnsubscribeResult,
};
use serde_json::Value;

use crate::agent::product_api::{
    AgentAuthenticateWorkflow, AgentCatalogMutationWorkflow, AgentProbeWorkflow,
    AgentSettingsDetailsWorkflow,
};
use crate::app_lifecycle::{AppLifecycle, InitializeAdmission, LifecycleState};
use crate::client_lifecycle::{AppServerTime, ClientHub, ConnectionId};
use crate::diagnostics::RuntimeDiagnosticsWorkflow;
use crate::projects::ConfiguredProjectRoots;
use crate::protocol::errors::RuntimeError;
use crate::server_requests::ServerRequestRuntime;
use crate::settings::{
    AppPreferencesWorkflow, McpServersSettingsWorkflow, RuntimeSettingsWorkflow,
    SkillsSettingsWorkflow,
};
use crate::shell_file_handles::ShellFileRevealRegistry;
use crate::snapshots::{SnapshotBuilder, TaskSnapshotSource};
use crate::state_sync::StateStream;
use crate::tasks::product_api::{
    AgentListSessionsWorkflow, AttachmentFileBrowserWorkflow, TaskFileSearchWorkflow,
    TaskOpenWorkflow, TaskReleaseWorkflow, TaskSetConfigOptionWorkflow,
};
use crate::tasks::product_api::{
    TaskAcquireWorkflow, TaskAdoptNativeSessionWorkflow, TaskArchiveWorkflow, TaskCancelWorkflow,
    TaskChatPageWorkflow, TaskSendWorkflow,
};

pub struct RpcGateway {
    pub(crate) client_hub: ClientHub,
    pub(crate) lifecycle: AppLifecycle,
    pub(crate) state_stream: StateStream,
    pub(crate) server_requests: ServerRequestRuntime,
    pub(crate) pending_event_deliveries: Vec<GatewayEventDelivery>,
    pub(crate) shell_file_reveals: ShellFileRevealRegistry,
    pub(crate) snapshots: SnapshotBuilder,
    pub(crate) task_snapshots: Arc<dyn TaskSnapshotSource>,
    project_roots: ConfiguredProjectRoots,
    probe_facts: AppServerProbeFacts,
    diagnostics: Arc<dyn RuntimeDiagnosticsWorkflow>,
    agent_probe: Arc<dyn AgentProbeWorkflow>,
    agent_authenticate: Arc<dyn AgentAuthenticateWorkflow>,
    agent_catalog_mutations: Arc<dyn AgentCatalogMutationWorkflow>,
    agent_settings_details: Arc<dyn AgentSettingsDetailsWorkflow>,
    mcp_servers_settings: Arc<dyn McpServersSettingsWorkflow>,
    skills_settings: Arc<dyn SkillsSettingsWorkflow>,
    app_preferences: Arc<dyn AppPreferencesWorkflow>,
    runtime_settings: Arc<dyn RuntimeSettingsWorkflow>,
    agent_list_sessions: Arc<dyn AgentListSessionsWorkflow>,
    attachments: Arc<dyn AttachmentFileBrowserWorkflow>,
    task_acquire: Arc<dyn TaskAcquireWorkflow>,
    task_file_search: Arc<dyn TaskFileSearchWorkflow>,
    task_adopt_native_session: Arc<dyn TaskAdoptNativeSessionWorkflow>,
    task_send: Arc<dyn TaskSendWorkflow>,
    task_cancel: Arc<dyn TaskCancelWorkflow>,
    task_open: Arc<dyn TaskOpenWorkflow>,
    task_chat_page: Arc<dyn TaskChatPageWorkflow>,
    task_set_config_option: Arc<dyn TaskSetConfigOptionWorkflow>,
    task_release: Arc<dyn TaskReleaseWorkflow>,
    task_archive: Arc<dyn TaskArchiveWorkflow>,
    worktrees: Arc<crate::worktrees::WorktreeManager>,
    shutdown: Arc<dyn AppServerShutdownWorkflow>,
}

pub(crate) trait AppServerShutdownWorkflow: Send + Sync {
    fn shutdown(&self) -> Result<(), RuntimeError>;
    fn shutdown_blockers(&self) -> Result<ShutdownBlockers, RuntimeError>;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ShutdownBlockers {
    pub active_turns: usize,
    pub pending_task_requests: usize,
}

impl ShutdownBlockers {
    pub(crate) fn is_empty(&self) -> bool {
        self.active_turns == 0 && self.pending_task_requests == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IdleShutdownDecision {
    ShutdownNow,
    KeepRunning {
        initialized_clients: bool,
        blockers: ShutdownBlockers,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppServerProbeFacts {
    pub state_root_fingerprint: String,
    pub protocol_version: String,
    pub app_version: String,
}

impl AppServerProbeFacts {
    pub fn new(state_root_fingerprint: impl Into<String>) -> Self {
        Self {
            state_root_fingerprint: state_root_fingerprint.into(),
            protocol_version: APP_SERVER_PROTOCOL_VERSION.to_string(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

impl RpcGateway {
    // This is the App Server composition root: dependencies remain explicit so
    // protocol handlers cannot acquire hidden global workflow state.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        client_hub: ClientHub,
        lifecycle: AppLifecycle,
        state_stream: StateStream,
        server_requests: ServerRequestRuntime,
        shell_file_reveals: ShellFileRevealRegistry,
        snapshots: SnapshotBuilder,
        task_snapshots: Arc<dyn TaskSnapshotSource>,
        project_roots: ConfiguredProjectRoots,
        probe_facts: AppServerProbeFacts,
        diagnostics: Arc<dyn RuntimeDiagnosticsWorkflow>,
        agent_probe: Arc<dyn AgentProbeWorkflow>,
        agent_authenticate: Arc<dyn AgentAuthenticateWorkflow>,
        agent_catalog_mutations: Arc<dyn AgentCatalogMutationWorkflow>,
        agent_settings_details: Arc<dyn AgentSettingsDetailsWorkflow>,
        mcp_servers_settings: Arc<dyn McpServersSettingsWorkflow>,
        skills_settings: Arc<dyn SkillsSettingsWorkflow>,
        app_preferences: Arc<dyn AppPreferencesWorkflow>,
        runtime_settings: Arc<dyn RuntimeSettingsWorkflow>,
        agent_list_sessions: Arc<dyn AgentListSessionsWorkflow>,
        attachments: Arc<dyn AttachmentFileBrowserWorkflow>,
        task_acquire: Arc<dyn TaskAcquireWorkflow>,
        task_file_search: Arc<dyn TaskFileSearchWorkflow>,
        task_adopt_native_session: Arc<dyn TaskAdoptNativeSessionWorkflow>,
        task_send: Arc<dyn TaskSendWorkflow>,
        task_cancel: Arc<dyn TaskCancelWorkflow>,
        task_open: Arc<dyn TaskOpenWorkflow>,
        task_chat_page: Arc<dyn TaskChatPageWorkflow>,
        task_set_config_option: Arc<dyn TaskSetConfigOptionWorkflow>,
        task_release: Arc<dyn TaskReleaseWorkflow>,
        task_archive: Arc<dyn TaskArchiveWorkflow>,
        worktrees: Arc<crate::worktrees::WorktreeManager>,
        shutdown: Arc<dyn AppServerShutdownWorkflow>,
    ) -> Self {
        Self {
            client_hub,
            lifecycle,
            state_stream,
            server_requests,
            pending_event_deliveries: Vec::new(),
            shell_file_reveals,
            snapshots,
            task_snapshots,
            project_roots,
            probe_facts,
            diagnostics,
            agent_probe,
            agent_authenticate,
            agent_catalog_mutations,
            agent_settings_details,
            mcp_servers_settings,
            skills_settings,
            app_preferences,
            runtime_settings,
            agent_list_sessions,
            attachments,
            task_acquire,
            task_file_search,
            task_adopt_native_session,
            task_send,
            task_cancel,
            task_open,
            task_chat_page,
            task_set_config_option,
            task_release,
            task_archive,
            worktrees,
            shutdown,
        }
    }

    fn handle_client_probe(
        &self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        if let Err(error) = serde_json::from_value::<ClientProbeParams>(params) {
            return self.error(connection_id, id, meta, responses::invalid_params(error));
        }
        self.result(
            connection_id,
            id,
            meta,
            ClientProbeResult {
                state_root_fingerprint: self.probe_facts.state_root_fingerprint.clone(),
                protocol_version: self.probe_facts.protocol_version.clone(),
                app_version: self.probe_facts.app_version.clone(),
                lifecycle: probe_lifecycle(self.lifecycle.state()),
            },
        )
    }

    pub(crate) fn probe_facts(&self) -> AppServerProbeFacts {
        self.probe_facts.clone()
    }

    fn handle_initialize(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<InitializeParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        if let InitializeAdmission::Rejected(error) = self.lifecycle.admit_initialize(now) {
            return self.error(connection_id, id, meta, error);
        }
        let projects_changed = self.project_roots.replace_client_workspace_roots(
            &params.client_instance_id,
            params.workspace_roots.iter().map(|root| root.path.clone()),
        );
        let outcome = self
            .client_hub
            .initialize(connection_id.clone(), params, now);
        let (context, reattached) = match outcome {
            crate::client_lifecycle::InitializeClientOutcome::NewClient { context } => {
                (context, false)
            }
            crate::client_lifecycle::InitializeClientOutcome::ReattachedClient { context } => {
                (context, true)
            }
        };
        if reattached {
            self.attachments
                .keep_alive_for_client(&context.client_instance_id);
        } else {
            self.attachments
                .discard_resources_for_client(&context.client_instance_id);
        }
        let server_requests = self
            .server_requests
            .observe_client_initialized_or_reattached(
                self.client_hub
                    .delivery_for(&context.client_instance_id)
                    .expect("initialized client must have a delivery"),
                &self.responder_scopes(&context),
                now,
            );
        let token = self
            .state_stream
            .read_token_for_client(&context.client_instance_id);
        let mut snapshot = match self.snapshots.client_snapshot(
            &context,
            context.requested_surface.clone(),
            &token,
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        snapshot.pending_requests = self
            .server_requests
            .pending_for_client(&context.client_instance_id);
        let events = if projects_changed {
            self.publish_project_collection_update(now)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        responses::result_with_events_and_server_requests(
            connection_id,
            id,
            meta,
            InitializeResult { snapshot },
            events,
            server_requests,
        )
    }

    fn handle_subscribe(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<StateSubscribeParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let Some(ctx) = self.client_hub.context_for_connection(&connection_id) else {
            return self.error(
                connection_id,
                id,
                meta,
                responses::not_initialized(STATE_SUBSCRIBE.to_string()),
            );
        };
        let mut result = match self
            .state_stream
            .subscribe(&ctx, params.scope, &self.snapshots, now)
        {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.add_pending_to_subscription_snapshot(&mut result.snapshot);
        let server_requests = match &result.scope {
            openaide_app_server_protocol::state::SubscriptionScope::Task { task_id } => {
                self.server_requests.observe_subscription_added(
                    self.client_hub
                        .delivery_for(&ctx.client_instance_id)
                        .expect("subscribed client must have a delivery"),
                    task_id.clone(),
                    now,
                )
            }
            _ => Vec::new(),
        };
        self.result_with_server_requests::<StateSubscribeResult>(
            connection_id,
            id,
            meta,
            result,
            server_requests,
        )
    }

    fn handle_unsubscribe(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<StateUnsubscribeParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let Some(ctx) = self.client_hub.context_for_connection(&connection_id) else {
            return self.error(
                connection_id,
                id,
                meta,
                responses::not_initialized(STATE_UNSUBSCRIBE.to_string()),
            );
        };
        let scope = params.scope.clone();
        let result = self.state_stream.unsubscribe(&ctx, params.scope, now);
        if let openaide_app_server_protocol::state::SubscriptionScope::Task { task_id } = &scope {
            self.server_requests.observe_subscription_removed(
                &ctx.client_instance_id,
                task_id,
                now,
            );
        }
        self.result::<StateUnsubscribeResult>(connection_id, id, meta, result)
    }

    fn result<T: serde::Serialize>(
        &self,
        connection_id: ConnectionId,
        id: String,
        meta: RequestMeta,
        result: T,
    ) -> GatewayOutcome {
        responses::result(connection_id, id, meta, result)
    }

    pub(super) fn error(
        &self,
        connection_id: ConnectionId,
        id: String,
        meta: RequestMeta,
        error: openaide_app_server_protocol::errors::ProtocolError,
    ) -> GatewayOutcome {
        responses::error(connection_id, id, meta, error)
    }
}

fn probe_lifecycle(state: LifecycleState) -> ClientProbeLifecycle {
    match state {
        LifecycleState::Running => ClientProbeLifecycle::Running,
        LifecycleState::Draining => ClientProbeLifecycle::Draining,
        LifecycleState::Stopping => ClientProbeLifecycle::Stopping,
    }
}

#[cfg(test)]
mod tests;
