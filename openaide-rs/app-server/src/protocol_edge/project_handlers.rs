use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::project::{
    ProjectMutationResult, ProjectReconnectParams, ProjectRegisterParams, ProjectRemoveParams,
    ProjectRenameParams,
};
use serde_json::Value;

use crate::client_lifecycle::{AppServerTime, ConnectionId};

use super::{responses, GatewayOutcome, RpcGateway};

impl RpcGateway {
    pub(super) fn handle_project_register(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<ProjectRegisterParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let result = match self.project_management.register_project(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.project_mutation_result(connection_id, id, meta, result, now)
    }

    pub(super) fn handle_project_rename(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<ProjectRenameParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let result = match self.project_management.rename_project(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.project_mutation_result(connection_id, id, meta, result, now)
    }

    pub(super) fn handle_project_reconnect(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<ProjectReconnectParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let result = match self.project_management.reconnect_project(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.project_mutation_result(connection_id, id, meta, result, now)
    }

    pub(super) fn handle_project_remove(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<ProjectRemoveParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let result = match self.project_management.remove_project(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.project_mutation_result(connection_id, id, meta, result, now)
    }

    fn project_mutation_result(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        meta: RequestMeta,
        result: ProjectMutationResult,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let events = self
            .publish_project_collection_update(now)
            .unwrap_or_default();
        responses::result_with_events(connection_id, id, meta, result, events)
    }
}
