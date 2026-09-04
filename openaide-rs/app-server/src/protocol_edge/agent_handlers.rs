use openaide_app_server_protocol::agent::{
    AgentAuthenticateParams, AgentAuthenticateResult, AgentCancelAuthenticateParams,
    AgentCancelAuthenticateResult, AgentCreateCustomParams, AgentCreateCustomResult,
    AgentDeleteCustomParams, AgentDeleteCustomResult, AgentListSessionsParams,
    AgentListSessionsResult, AgentLogoutParams, AgentLogoutResult, AgentProbeParams,
    AgentProbeResult, AgentReplaceCustomParams, AgentReplaceCustomResult, AgentSetEnabledParams,
    AgentSetEnabledResult, AgentSettingsDetailsParams, AgentSettingsDetailsResult,
    AgentUpdateCustomMetadataParams, AgentUpdateCustomMetadataResult,
};
use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::events::{AppServerEventPayload, EventScope};
use openaide_app_server_protocol::snapshot::AgentCollectionSnapshot;
use openaide_app_server_protocol::task::{
    NativeSessionArchiveParams, NativeSessionArchiveResult, NativeSessionForkParams,
    NativeSessionForkResult, NativeSessionRestoreParams, NativeSessionRestoreResult,
    NativeSessionSetPinnedParams, NativeSessionSetPinnedResult, NativeSessionSetTitleParams,
    NativeSessionSetTitleResult, TaskNavigationLoadMoreParams, TaskNavigationLoadMoreResult,
    TaskNavigationRefreshParams, TaskNavigationRefreshResult,
};
use serde_json::Value;
use std::time::Instant;

use crate::agent::product_api::AgentAuthenticateWorkflow;
use crate::client_lifecycle::{AppServerTime, ConnectionId};
use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::methods::{AGENT_AUTHENTICATE, AGENT_LOGOUT};
use std::sync::Arc;

use super::{event_deliveries, responses, GatewayEventDelivery, GatewayOutcome, RpcGateway};

impl RpcGateway {
    pub(super) fn handle_native_session_set_title(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<NativeSessionSetTitleParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let mutation = match self.task_archive.set_native_session_title(params) {
            Ok(mutation) => mutation,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        let events = self.publish_project_entries_replaced(&mutation.project_id, now);
        self.result_with_events::<NativeSessionSetTitleResult>(
            connection_id,
            id,
            meta,
            NativeSessionSetTitleResult {
                session: mutation.session,
            },
            events,
        )
    }

    pub(super) fn handle_native_session_set_pinned(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<NativeSessionSetPinnedParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let mutation = match self.task_archive.set_native_session_pinned(params) {
            Ok(mutation) => mutation,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        let events = self.publish_project_entries_replaced(&mutation.project_id, now);
        self.result_with_events::<NativeSessionSetPinnedResult>(
            connection_id,
            id,
            meta,
            NativeSessionSetPinnedResult {
                session: mutation.session,
            },
            events,
        )
    }

    pub(super) fn handle_native_session_archive(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<NativeSessionArchiveParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let mutation = match self.task_archive.archive_native_session(params) {
            Ok(mutation) => mutation,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        let events = self.publish_project_entries_replaced(&mutation.project_id, now);
        self.result_with_events::<NativeSessionArchiveResult>(
            connection_id,
            id,
            meta,
            NativeSessionArchiveResult {
                reference: mutation.reference,
                archived: mutation.archived,
            },
            events,
        )
    }

    pub(super) fn handle_native_session_restore(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<NativeSessionRestoreParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let mutation = match self.task_archive.restore_native_session(params) {
            Ok(mutation) => mutation,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        let events = self.publish_project_entries_replaced(&mutation.project_id, now);
        self.result_with_events::<NativeSessionRestoreResult>(
            connection_id,
            id,
            meta,
            NativeSessionRestoreResult {
                reference: mutation.reference,
                archived: mutation.archived,
            },
            events,
        )
    }

    pub(super) fn handle_native_session_fork(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<NativeSessionForkParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for Native Session fork");
        let mutation = match self
            .task_archive
            .fork_native_session_for_client(&client.client_instance_id, params)
        {
            Ok(mutation) => mutation,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        let events = self.publish_project_entries_replaced(&mutation.project_id, now);
        self.result_with_events::<NativeSessionForkResult>(
            connection_id,
            id,
            meta,
            NativeSessionForkResult {
                reference: mutation.reference,
                project_id: mutation.project_id,
                close_warning: mutation.close_warning,
            },
            events,
        )
    }

    pub(super) fn handle_task_navigation_refresh(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        if let Err(error) = serde_json::from_value::<TaskNavigationRefreshParams>(params) {
            return self.error(connection_id, id, meta, responses::invalid_params(error));
        }
        self.agent_list_sessions
            .request_native_session_catalog_refresh();
        self.result(
            connection_id,
            id,
            meta,
            TaskNavigationRefreshResult { accepted: true },
        )
    }

    pub(super) fn handle_task_navigation_load_more(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<TaskNavigationLoadMoreParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        self.agent_list_sessions
            .request_native_session_catalog_load_more(
                params.project_id.as_str(),
                params.target_row_count as usize,
            );
        self.result(
            connection_id,
            id,
            meta,
            TaskNavigationLoadMoreResult { accepted: true },
        )
    }

    pub(super) fn handle_agent_probe(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<AgentProbeParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error));
            }
        };
        let result = match self.agent_probe.probe(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        let events = self.publish_agent_collection_update(result.agents.clone(), now);
        self.result_with_events::<AgentProbeResult>(connection_id, id, meta, result, events)
    }

    pub(super) fn handle_agent_list_sessions(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<AgentListSessionsParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error));
            }
        };
        let result = match self.agent_list_sessions.list_agent_sessions(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<AgentListSessionsResult>(connection_id, id, meta, result)
    }

    pub(super) fn handle_agent_authenticate(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        match self.prepare_agent_authenticate(
            connection_id.clone(),
            id.clone(),
            params,
            meta.clone(),
            now,
        ) {
            Err(outcome) => *outcome,
            Ok((params, workflow)) => self.finish_agent_authenticate(
                connection_id,
                id,
                meta,
                now,
                workflow.authenticate(params),
            ),
        }
    }

    /// Admission and parse only. The ChatGPT/device-code ACP call must run without
    /// holding the protocol lock so heartbeats and host `shell/openExternal` can proceed.
    pub(crate) fn prepare_agent_authenticate(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> Result<(AgentAuthenticateParams, Arc<dyn AgentAuthenticateWorkflow>), Box<GatewayOutcome>>
    {
        if self
            .client_hub
            .context_for_connection(&connection_id)
            .is_none()
        {
            return Err(Box::new(self.error(
                connection_id,
                id,
                meta,
                responses::not_initialized(AGENT_AUTHENTICATE.to_string()),
            )));
        }
        if let Some(client_instance_id) = self
            .client_hub
            .observe_connection_activity(&connection_id, now)
        {
            self.attachments.keep_alive_for_client(&client_instance_id);
        }
        if self.update_shutdown.is_some() {
            return Err(Box::new(self.error(
                connection_id,
                id,
                meta,
                responses::update_shutdown_in_progress(AGENT_AUTHENTICATE.to_string()),
            )));
        }
        let params = match serde_json::from_value::<AgentAuthenticateParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return Err(Box::new(self.error(
                    connection_id,
                    id,
                    meta,
                    responses::invalid_params(error),
                )));
            }
        };
        Ok((params, self.agent_authenticate.clone()))
    }

    pub(crate) fn finish_agent_authenticate(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        meta: RequestMeta,
        now: AppServerTime,
        result: Result<AgentAuthenticateResult, ProtocolError>,
    ) -> GatewayOutcome {
        let result = match result {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        let events = self.publish_agent_collection_update(result.agents.clone(), now);
        self.result_with_events::<AgentAuthenticateResult>(connection_id, id, meta, result, events)
    }

    /// Admission and parse only. ACP logout may wait on the Agent process, so it runs without
    /// holding the shared protocol lock.
    pub(crate) fn prepare_agent_logout(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> Result<(AgentLogoutParams, Arc<dyn AgentAuthenticateWorkflow>), Box<GatewayOutcome>> {
        if self
            .client_hub
            .context_for_connection(&connection_id)
            .is_none()
        {
            return Err(Box::new(self.error(
                connection_id,
                id,
                meta,
                responses::not_initialized(AGENT_LOGOUT.to_string()),
            )));
        }
        if let Some(client_instance_id) = self
            .client_hub
            .observe_connection_activity(&connection_id, now)
        {
            self.attachments.keep_alive_for_client(&client_instance_id);
        }
        if self.update_shutdown.is_some() {
            return Err(Box::new(self.error(
                connection_id,
                id,
                meta,
                responses::update_shutdown_in_progress(AGENT_LOGOUT.to_string()),
            )));
        }
        let params = serde_json::from_value::<AgentLogoutParams>(params).map_err(|error| {
            Box::new(self.error(connection_id, id, meta, responses::invalid_params(error)))
        })?;
        Ok((params, self.agent_authenticate.clone()))
    }

    pub(crate) fn finish_agent_logout(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        meta: RequestMeta,
        now: AppServerTime,
        result: Result<AgentLogoutResult, ProtocolError>,
    ) -> GatewayOutcome {
        let result = match result {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        let events = self.publish_agent_collection_update(result.agents.clone(), now);
        self.result_with_events::<AgentLogoutResult>(connection_id, id, meta, result, events)
    }

    pub(super) fn handle_agent_cancel_authenticate(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<AgentCancelAuthenticateParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error));
            }
        };
        let result = match self.agent_authenticate.cancel_authenticate(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        let events = self.publish_agent_collection_update(result.agents.clone(), now);
        self.result_with_events::<AgentCancelAuthenticateResult>(
            connection_id,
            id,
            meta,
            result,
            events,
        )
    }

    pub(super) fn handle_agent_create_custom(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<AgentCreateCustomParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error));
            }
        };
        let result = match self.agent_catalog_mutations.create_custom(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        let events = self.publish_agent_collection_update(result.agents.clone(), now);
        self.result_with_events::<AgentCreateCustomResult>(connection_id, id, meta, result, events)
    }

    pub(super) fn handle_agent_update_custom_metadata(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<AgentUpdateCustomMetadataParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error));
            }
        };
        let disabled_agent_id = (!params.enabled).then(|| params.agent_id.clone());
        let result = match self.agent_catalog_mutations.update_custom_metadata(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        if let Some(agent_id) = disabled_agent_id {
            self.dispose_prepared_tasks_after_agent_mutation(
                agent_id.as_str(),
                "agent/updateCustomMetadata",
            );
        }
        let events = self.publish_agent_collection_update(result.agents.clone(), now);
        self.result_with_events::<AgentUpdateCustomMetadataResult>(
            connection_id,
            id,
            meta,
            result,
            events,
        )
    }

    pub(super) fn handle_agent_replace_custom(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<AgentReplaceCustomParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error));
            }
        };
        let replaced_agent_id = params.source_agent_id.clone();
        let result = match self.agent_catalog_mutations.replace_custom(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.dispose_prepared_tasks_after_agent_mutation(
            replaced_agent_id.as_str(),
            "agent/replaceCustom",
        );
        let events = self.publish_agent_collection_update(result.agents.clone(), now);
        self.result_with_events::<AgentReplaceCustomResult>(connection_id, id, meta, result, events)
    }

    pub(super) fn handle_agent_delete_custom(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<AgentDeleteCustomParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error));
            }
        };
        let deleted_agent_id = params.agent_id.clone();
        let result = match self.agent_catalog_mutations.delete_custom(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.dispose_prepared_tasks_after_agent_mutation(
            deleted_agent_id.as_str(),
            "agent/deleteCustom",
        );
        let events = self.publish_agent_collection_update(result.agents.clone(), now);
        self.result_with_events::<AgentDeleteCustomResult>(connection_id, id, meta, result, events)
    }

    pub(super) fn handle_agent_set_enabled(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<AgentSetEnabledParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error));
            }
        };
        let disabled_agent_id = (!params.enabled).then(|| params.agent_id.clone());
        let result = match self.agent_catalog_mutations.set_enabled(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        if let Some(agent_id) = disabled_agent_id {
            self.dispose_prepared_tasks_after_agent_mutation(agent_id.as_str(), "agent/setEnabled");
        }
        let events = self.publish_agent_collection_update(result.agents.clone(), now);
        self.result_with_events::<AgentSetEnabledResult>(connection_id, id, meta, result, events)
    }

    pub(super) fn handle_settings_get_agent_details(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<AgentSettingsDetailsParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error));
            }
        };
        let result = match self.agent_settings_details.agent_settings_details(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<AgentSettingsDetailsResult>(connection_id, id, meta, result)
    }

    fn publish_agent_collection_update(
        &mut self,
        agents: AgentCollectionSnapshot,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        let client_hub = self.client_hub.clone();
        let mut events = event_deliveries(self.state_stream.publish_committed(
            EventScope::StateRoot {
                state_root_id: self.state_stream.state_root_id().clone(),
            },
            AppServerEventPayload::AgentCollectionUpdated { agents },
            |client_id| client_hub.delivery_for(client_id),
            now,
        ));
        events.extend(self.publish_navigation_replacement(now));
        events
    }

    pub(crate) fn publish_background_agent_status_update(
        &mut self,
        now: AppServerTime,
    ) -> Vec<GatewayEventDelivery> {
        // Provisioning activity is an Agent Status transition. Reuse the
        // coalesced discovery owner so session history appears without a
        // frontend-authored retry or a prompt replay.
        self.agent_list_sessions
            .request_native_session_catalog_refresh();
        let events = match self.snapshots.agent_collection_snapshot() {
            Ok(agents) => {
                let installing_agent_count = agents
                    .agents
                    .iter()
                    .filter(|agent| {
                        agent.status
                            == openaide_app_server_protocol::snapshot::AgentStatus::Installing
                    })
                    .count();
                let events = self.publish_agent_collection_update(agents, now);
                if installing_agent_count > 0 {
                    crate::logging::info(
                        "agent_installation_status_published",
                        serde_json::json!({
                            "installing_agent_count": installing_agent_count,
                            "event_count": events.len(),
                        }),
                    );
                }
                events
            }
            Err(_) => Vec::new(),
        };
        // Local HTTP clients drain background publications on their next poll;
        // stdio callers also receive this returned copy for immediate delivery.
        self.pending_event_deliveries.extend(events.clone());
        events
    }

    fn dispose_prepared_tasks_after_agent_mutation(&self, agent_id: &str, operation: &str) {
        // The catalog mutation is already durable and authoritative. Cleanup
        // failure must not turn that committed user action into an RPC error;
        // stale Prepared Tasks contain no accepted user history and remain recoverable. The
        // cleanup can close an unresponsive ACP process, so it must not hold up the mutation RPC.
        let task_release = self.task_release.clone();
        let agent_id = agent_id.to_string();
        let operation = operation.to_string();
        let log_agent_id = agent_id.clone();
        let log_operation = operation.clone();
        let spawn = std::thread::Builder::new()
            .name("openaide-agent-cleanup".to_string())
            .spawn(move || {
                let started_at = Instant::now();
                crate::logging::info(
                    "prepared_task_cleanup_started",
                    serde_json::json!({
                        "operation": operation.clone(),
                        "agent_id": agent_id.clone(),
                    }),
                );
                let succeeded = task_release
                    .dispose_prepared_tasks_for_agent(agent_id.as_str())
                    .is_ok();
                crate::logging::info(
                    "prepared_task_cleanup_completed",
                    serde_json::json!({
                        "operation": operation.clone(),
                        "agent_id": agent_id.clone(),
                        "outcome": if succeeded { "succeeded" } else { "failed" },
                        "duration_ms": started_at.elapsed().as_millis(),
                    }),
                );
            });
        if spawn.is_err() {
            crate::logging::warn(
                "prepared_task_cleanup_spawn_failed",
                serde_json::json!({
                    "operation": log_operation,
                    "agent_id": log_agent_id,
                }),
            );
        }
    }
}
