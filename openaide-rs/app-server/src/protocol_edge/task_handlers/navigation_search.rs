use std::sync::Arc;

use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::ClientInstanceId;
use openaide_app_server_protocol::task::{TaskNavigationSearchParams, TaskNavigationSearchResult};
use serde_json::Value;

use crate::client_lifecycle::ConnectionId;
use crate::navigation_search::NavigationSearchWorkflow;
use crate::protocol_edge::{responses, GatewayOutcome};

use super::RpcGateway;

/// A `taskNavigation/search` request whose params and caller are checked. The scan reads
/// whole Chats, so it runs after the protocol lock is released.
pub(in crate::protocol_edge) struct PreparedNavigationSearch {
    pub owner: ClientInstanceId,
    params: TaskNavigationSearchParams,
    search: Arc<dyn NavigationSearchWorkflow>,
}

impl PreparedNavigationSearch {
    pub fn run(self) -> Result<TaskNavigationSearchResult, ProtocolError> {
        self.search.search(self.params)
    }
}

impl RpcGateway {
    pub(in crate::protocol_edge) fn handle_task_navigation_search(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        match self.prepare_navigation_search(&connection_id, &id, params, &meta) {
            Ok(prepared) => {
                let result = prepared.run();
                self.answer_navigation_search(connection_id, id, meta, result)
            }
            Err(outcome) => *outcome,
        }
    }

    pub(in crate::protocol_edge) fn answer_navigation_search(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        meta: RequestMeta,
        result: Result<TaskNavigationSearchResult, ProtocolError>,
    ) -> GatewayOutcome {
        match result {
            Ok(result) => {
                self.result::<TaskNavigationSearchResult>(connection_id, id, meta, result)
            }
            Err(error) => self.error(connection_id, id, meta, error),
        }
    }

    pub(in crate::protocol_edge) fn prepare_navigation_search(
        &mut self,
        connection_id: &ConnectionId,
        id: &str,
        params: Value,
        meta: &RequestMeta,
    ) -> Result<PreparedNavigationSearch, Box<GatewayOutcome>> {
        let params = match serde_json::from_value::<TaskNavigationSearchParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return Err(Box::new(self.error(
                    connection_id.clone(),
                    id.to_string(),
                    meta.clone(),
                    responses::invalid_params(error),
                )))
            }
        };
        let client = self
            .client_hub
            .context_for_connection(connection_id)
            .expect("routing requires an initialized client for search");
        Ok(PreparedNavigationSearch {
            owner: client.client_instance_id,
            params,
            search: self.navigation_search.clone(),
        })
    }
}
