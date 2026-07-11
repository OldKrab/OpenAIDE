use openaide_app_server_protocol::diagnostics::{
    RuntimeDiagnosticsParams, RuntimeDiagnosticsResult,
};
use openaide_app_server_protocol::envelopes::RequestMeta;
use serde_json::Value;

use crate::client_lifecycle::ConnectionId;

use super::{responses, GatewayOutcome, RpcGateway};

impl RpcGateway {
    pub(super) fn handle_diagnostics_get_runtime(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        if let Err(error) = serde_json::from_value::<RuntimeDiagnosticsParams>(params) {
            return self.error(connection_id, id, meta, responses::invalid_params(error));
        }
        let result = match self.diagnostics.runtime_diagnostics() {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<RuntimeDiagnosticsResult>(connection_id, id, meta, result)
    }
}
