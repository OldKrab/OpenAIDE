use openaide_app_server_protocol::attachment::{
    AttachmentConfirmEmbeddedParams, AttachmentConfirmEmbeddedResult,
    AttachmentCreateEmbeddedCandidateParams, AttachmentCreateEmbeddedCandidateResult,
    AttachmentCreateFileReferenceParams, AttachmentCreateFileReferenceResult,
    AttachmentCreatePastedImageParams, AttachmentCreatePastedImageResult,
    AttachmentListDirectoryParams, AttachmentListDirectoryResult, AttachmentListRootsParams,
    AttachmentListRootsResult, AttachmentRefreshHandlesParams, AttachmentRefreshHandlesResult,
    AttachmentReleaseParams, AttachmentReleaseResult, AttachmentRevealParams,
    AttachmentRevealResult,
};
use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::server_requests::{ShellRevealFileParams, SHELL_REVEAL_FILE};
use openaide_app_server_protocol::snapshot::PendingRequestScope;
use openaide_app_server_protocol::workspace::{
    WorkspaceListDirectoryParams, WorkspaceListDirectoryResult, WorkspaceListRootsParams,
    WorkspaceListRootsResult,
};
use serde_json::Value;

use crate::client_lifecycle::ConnectionId;
use crate::protocol::errors::RuntimeError;
use crate::server_requests::{OpenRequestOutcome, ServerRequestDraft};

use super::{responses, GatewayOutcome, RpcGateway};

impl RpcGateway {
    pub(super) fn handle_attachment_list_roots(
        &self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<AttachmentListRootsParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for attachment roots");
        let result = match self
            .attachments
            .list_roots(&client.client_instance_id, params)
        {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<AttachmentListRootsResult>(connection_id, id, meta, result)
    }

    pub(super) fn handle_attachment_list_directory(
        &self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<AttachmentListDirectoryParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for attachment browsing");
        let result = match self
            .attachments
            .list_directory(&client.client_instance_id, params)
        {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<AttachmentListDirectoryResult>(connection_id, id, meta, result)
    }

    pub(super) fn handle_attachment_create_file_reference(
        &self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<AttachmentCreateFileReferenceParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for attachment creation");
        let result = match self
            .attachments
            .create_file_reference(&client.client_instance_id, params)
        {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<AttachmentCreateFileReferenceResult>(connection_id, id, meta, result)
    }

    pub(super) fn handle_attachment_create_pasted_image(
        &self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<AttachmentCreatePastedImageParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for attachment creation");
        let result = match self
            .attachments
            .create_pasted_image(&client.client_instance_id, params)
        {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<AttachmentCreatePastedImageResult>(connection_id, id, meta, result)
    }

    pub(super) fn handle_attachment_create_embedded_candidate(
        &self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<AttachmentCreateEmbeddedCandidateParams>(params)
        {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for attachment creation");
        let result = match self
            .attachments
            .create_embedded_candidate(&client.client_instance_id, params)
        {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<AttachmentCreateEmbeddedCandidateResult>(connection_id, id, meta, result)
    }

    pub(super) fn handle_attachment_confirm_embedded(
        &self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<AttachmentConfirmEmbeddedParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for attachment confirmation");
        let result = match self
            .attachments
            .confirm_embedded(&client.client_instance_id, params)
        {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<AttachmentConfirmEmbeddedResult>(connection_id, id, meta, result)
    }

    pub(super) fn handle_attachment_refresh_handles(
        &self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<AttachmentRefreshHandlesParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for attachment refresh");
        let result = match self
            .attachments
            .refresh_handles(&client.client_instance_id, params)
        {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<AttachmentRefreshHandlesResult>(connection_id, id, meta, result)
    }

    pub(super) fn handle_attachment_release(
        &self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<AttachmentReleaseParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for attachment release");
        let result = match self
            .attachments
            .release_resources(&client.client_instance_id, params)
        {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<AttachmentReleaseResult>(connection_id, id, meta, result)
    }

    pub(super) fn handle_attachment_reveal(
        &self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: crate::client_lifecycle::AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<AttachmentRevealParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let Some(client) = self.client_hub.context_for_connection(&connection_id) else {
            return self.error(
                connection_id,
                id,
                meta,
                reveal_unavailable("client is unavailable"),
            );
        };
        let Some(delivery) = self.client_hub.delivery_for(&client.client_instance_id) else {
            return self.error(
                client.connection_id,
                id,
                meta,
                reveal_unavailable("client is unavailable"),
            );
        };
        let target = match self
            .attachments
            .resolve_reveal_target(&client.client_instance_id, params)
        {
            Ok(target) => target,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        let reveal_handle = match self.shell_file_reveals.register_local_file_for_client(
            client.client_instance_id.clone(),
            target.path,
            Some(target.label.clone()),
        ) {
            Ok(handle) => handle,
            Err(error) => return self.error(connection_id, id, meta, reveal_runtime_error(error)),
        };
        let request_params = match serde_json::to_value(ShellRevealFileParams {
            originating_client_instance_id: client.client_instance_id.clone(),
            file_handle_id: reveal_handle.id,
            label: Some(reveal_handle.label),
        }) {
            Ok(params) => params,
            Err(error) => {
                return self.error(
                    connection_id,
                    id,
                    meta,
                    reveal_unavailable(&format!("reveal request could not be serialized: {error}")),
                )
            }
        };
        let opened = self.server_requests.open(
            ServerRequestDraft {
                scope: PendingRequestScope::Client {
                    client_instance_id: client.client_instance_id,
                },
                method: SHELL_REVEAL_FILE.to_string(),
                title: "Reveal file".to_string(),
                params: request_params,
            },
            vec![delivery],
            now,
        );
        let OpenRequestOutcome::Opened { deliveries, .. } = opened else {
            return self.error(
                connection_id,
                id,
                meta,
                reveal_unavailable("reveal request is unavailable"),
            );
        };
        responses::result_with_server_requests(
            connection_id,
            id,
            meta,
            AttachmentRevealResult { requested: true },
            deliveries,
        )
    }

    pub(super) fn handle_workspace_list_roots(
        &self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<WorkspaceListRootsParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let result = match self.attachments.workspace_roots(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<WorkspaceListRootsResult>(connection_id, id, meta, result)
    }

    pub(super) fn handle_workspace_list_directory(
        &self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<WorkspaceListDirectoryParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let result = match self.attachments.workspace_directory(params) {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<WorkspaceListDirectoryResult>(connection_id, id, meta, result)
    }
}

fn reveal_runtime_error(error: RuntimeError) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::InvalidRequest,
        message: error.to_string(),
        recoverable: true,
        target: None,
    }
}

fn reveal_unavailable(message: &str) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::CapabilityUnavailable,
        message: message.to_string(),
        recoverable: true,
        target: None,
    }
}
