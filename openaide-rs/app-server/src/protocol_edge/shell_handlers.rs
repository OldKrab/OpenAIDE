use openaide_app_server_protocol::client::{ShellCapability, ShellKind};
use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::server_requests::{
    ShellResolveFileRevealParams, ShellResolveFileRevealResult,
};
use serde_json::Value;

use crate::client_lifecycle::ConnectionId;

use super::{responses, GatewayOutcome, RpcGateway};

impl RpcGateway {
    pub(super) fn handle_shell_resolve_file_reveal(
        &self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<ShellResolveFileRevealParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for file reveal resolution");
        if client.shell.kind != ShellKind::VscodeExtension
            || !client
                .capabilities
                .shell
                .contains(&ShellCapability::ResolveFileReveal)
        {
            return self.error(
                connection_id,
                id,
                meta,
                resolver_unavailable("client cannot resolve native VS Code file reveal handles"),
            );
        }
        let Some(target) = self.shell_file_reveals.consume_for_client(
            &params.originating_client_instance_id,
            &params.file_handle_id,
        ) else {
            return self.error(
                connection_id,
                id,
                meta,
                resolver_unavailable("file reveal handle is unavailable for originating client"),
            );
        };
        self.result::<ShellResolveFileRevealResult>(
            connection_id,
            id,
            meta,
            ShellResolveFileRevealResult {
                path: target.path.to_string_lossy().to_string(),
                label: target.label,
            },
        )
    }
}

fn resolver_unavailable(message: &str) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::CapabilityUnavailable,
        message: message.to_string(),
        recoverable: true,
        target: None,
    }
}
