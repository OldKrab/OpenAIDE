use openaide_app_server_protocol::agent::{
    AgentAuthenticateParams, AgentAuthenticateResult, AgentCreateCustomParams,
    AgentCreateCustomResult, AgentDeleteCustomParams, AgentDeleteCustomResult,
    AgentListSessionsParams, AgentListSessionsResult, AgentProbeParams, AgentProbeResult,
    AgentReplaceCustomParams, AgentReplaceCustomResult, AgentSetEnabledParams,
    AgentSetEnabledResult, AgentSettingsDetailsParams, AgentSettingsDetailsResult,
    AgentUpdateCustomMetadataParams, AgentUpdateCustomMetadataResult,
};
use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::events::{AppServerEventPayload, EventScope};
use openaide_app_server_protocol::snapshot::AgentCollectionSnapshot;
use serde_json::Value;

use crate::client_lifecycle::{AppServerTime, ConnectionId};

use super::{event_deliveries, responses, GatewayEventDelivery, GatewayOutcome, RpcGateway};

impl RpcGateway {
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
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<AgentAuthenticateParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error));
            }
        };
        let result = match self.agent_authenticate.authenticate(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<AgentAuthenticateResult>(connection_id, id, meta, result)
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
        let result = match self.agent_catalog_mutations.update_custom_metadata(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
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
        let result = match self.agent_catalog_mutations.replace_custom(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
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
        let result = match self.agent_catalog_mutations.delete_custom(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
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
        let result = match self.agent_catalog_mutations.set_enabled(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
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
        event_deliveries(self.state_stream.publish_committed(
            EventScope::StateRoot {
                state_root_id: self.state_stream.state_root_id().clone(),
            },
            AppServerEventPayload::AgentCollectionUpdated { agents },
            |client_id| client_hub.delivery_for(client_id),
            now,
        ))
    }
}
