use openaide_app_server_protocol::client::{
    ClientCapabilitiesChangedParams, ClientCapabilitiesChangedResult, ClientDetachParams,
    ClientDetachResult, ClientHeartbeatParams, ClientHeartbeatResult, UpdateShutdownAbortParams,
    UpdateShutdownAbortResult, UpdateShutdownBlockedReason, UpdateShutdownCommitParams,
    UpdateShutdownCommitResult, UpdateShutdownPrepareParams, UpdateShutdownPrepareResult,
};
use openaide_app_server_protocol::envelopes::RequestMeta;
use serde_json::Value;

use crate::client_lifecycle::{AppServerTime, ConnectionId};

use super::{responses, GatewayOutcome, RpcGateway, UpdateShutdownBarrier};

impl RpcGateway {
    /// Authenticated transport activity proves the initialized client is still live.
    pub(crate) fn observe_connection_activity(
        &mut self,
        connection_id: &ConnectionId,
        now: AppServerTime,
    ) -> bool {
        let Some(client_instance_id) = self
            .client_hub
            .observe_connection_activity(connection_id, now)
        else {
            return false;
        };
        self.attachments.keep_alive_for_client(&client_instance_id);
        true
    }

    pub(super) fn handle_client_capabilities_changed(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<ClientCapabilitiesChangedParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error));
            }
        };
        let Some(context) = self.client_hub.context_for_connection(&connection_id) else {
            return self.error(
                connection_id,
                id,
                meta,
                responses::not_initialized(
                    openaide_app_server_protocol::methods::CLIENT_CAPABILITIES_CHANGED.to_string(),
                ),
            );
        };

        if let Some(capabilities) = params.capabilities {
            self.client_hub
                .update_capabilities(&context.client_instance_id, capabilities);
        }
        let projects_changed = params.workspace_roots.is_some_and(|roots| {
            self.project_roots.replace_client_workspace_roots(
                &context.client_instance_id,
                roots.into_iter().map(|root| root.path),
            )
        });
        let projects = match self.snapshots.project_collection_snapshot() {
            Ok(projects) => projects,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        let events = if projects_changed {
            self.publish_project_collection_update(now)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        self.result_with_events(
            connection_id,
            id,
            meta,
            ClientCapabilitiesChangedResult { projects },
            events,
        )
    }

    pub(super) fn handle_client_heartbeat(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        _now: AppServerTime,
    ) -> GatewayOutcome {
        if let Err(error) = serde_json::from_value::<ClientHeartbeatParams>(params) {
            return self.error(connection_id, id, meta, responses::invalid_params(error));
        }
        let events = self.drain_event_deliveries_for_connection(&connection_id);
        responses::result_with_events(connection_id, id, meta, ClientHeartbeatResult {}, events)
    }

    /// Explicit host shutdown bypasses reconnect grace so the last VS Code closes the server.
    pub(super) fn handle_client_detach(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        if let Err(error) = serde_json::from_value::<ClientDetachParams>(params) {
            return self.error(connection_id, id, meta, responses::invalid_params(error));
        }
        let Some(context) = self.client_hub.context_for_connection(&connection_id) else {
            return self.error(
                connection_id,
                id,
                meta,
                responses::not_initialized(
                    openaide_app_server_protocol::methods::CLIENT_DETACH.to_string(),
                ),
            );
        };
        self.detach_client(&context.client_instance_id, now);
        responses::result(connection_id, id, meta, ClientDetachResult {})
    }

    pub(super) fn handle_update_shutdown_prepare(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<UpdateShutdownPrepareParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        if !valid_update_attempt_id(&params.attempt_id) {
            return self.error(
                connection_id,
                id,
                meta,
                responses::invalid_update_shutdown_attempt(),
            );
        }
        let Some(context) = self.client_hub.context_for_connection(&connection_id) else {
            return self.error(
                connection_id,
                id,
                meta,
                responses::not_initialized(
                    openaide_app_server_protocol::methods::CLIENT_UPDATE_SHUTDOWN_PREPARE
                        .to_string(),
                ),
            );
        };

        if let Some(barrier) = &self.update_shutdown {
            let result = if barrier.owner == context.client_instance_id
                && barrier.attempt_id == params.attempt_id
            {
                UpdateShutdownPrepareResult::Ready
            } else {
                UpdateShutdownPrepareResult::Blocked {
                    reason: UpdateShutdownBlockedReason::OtherClients,
                }
            };
            return responses::result(connection_id, id, meta, result);
        }

        if self
            .client_hub
            .has_other_initialized_client(&context.client_instance_id)
        {
            return responses::result(
                connection_id,
                id,
                meta,
                UpdateShutdownPrepareResult::Blocked {
                    reason: UpdateShutdownBlockedReason::OtherClients,
                },
            );
        }
        let blockers = match self.shutdown.shutdown_blockers() {
            Ok(blockers) => blockers,
            Err(_error) => {
                return self.error(
                    connection_id,
                    id,
                    meta,
                    responses::update_shutdown_readiness_failed(),
                )
            }
        };
        if !params.stop_active_work
            && (blockers.active_turns > 0 || blockers.pending_task_requests > 0)
        {
            return responses::result(
                connection_id,
                id,
                meta,
                UpdateShutdownPrepareResult::Blocked {
                    reason: UpdateShutdownBlockedReason::ActiveWork,
                },
            );
        }

        self.update_shutdown = Some(UpdateShutdownBarrier {
            owner: context.client_instance_id,
            attempt_id: params.attempt_id,
            committed: false,
        });
        responses::result(connection_id, id, meta, UpdateShutdownPrepareResult::Ready)
    }

    pub(super) fn handle_update_shutdown_commit(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<UpdateShutdownCommitParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let Some(context) = self.client_hub.context_for_connection(&connection_id) else {
            return self.error(
                connection_id,
                id,
                meta,
                responses::not_initialized(
                    openaide_app_server_protocol::methods::CLIENT_UPDATE_SHUTDOWN_COMMIT
                        .to_string(),
                ),
            );
        };
        let Some(barrier) = self.update_shutdown.as_mut() else {
            return self.error(
                connection_id,
                id,
                meta,
                responses::invalid_update_shutdown_attempt(),
            );
        };
        if barrier.owner != context.client_instance_id || barrier.attempt_id != params.attempt_id {
            return self.error(
                connection_id,
                id,
                meta,
                responses::invalid_update_shutdown_attempt(),
            );
        }
        if !barrier.committed {
            barrier.committed = true;
        }
        responses::result(connection_id, id, meta, UpdateShutdownCommitResult {})
    }

    pub(super) fn handle_update_shutdown_abort(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<UpdateShutdownAbortParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let Some(context) = self.client_hub.context_for_connection(&connection_id) else {
            return self.error(
                connection_id,
                id,
                meta,
                responses::not_initialized(
                    openaide_app_server_protocol::methods::CLIENT_UPDATE_SHUTDOWN_ABORT.to_string(),
                ),
            );
        };
        let owned = self.update_shutdown.as_ref().is_some_and(|barrier| {
            barrier.owner == context.client_instance_id && barrier.attempt_id == params.attempt_id
        });
        if !owned {
            return self.error(
                connection_id,
                id,
                meta,
                responses::invalid_update_shutdown_attempt(),
            );
        }
        self.update_shutdown = None;
        responses::result(connection_id, id, meta, UpdateShutdownAbortResult {})
    }
}

fn valid_update_attempt_id(attempt_id: &str) -> bool {
    !attempt_id.is_empty() && attempt_id.len() <= 128
}
