//! Upload registration: turns a completed upload-route file into a pre-Send handle.

use openaide_app_server_protocol::attachment::PreSendAttachment;
use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::{ClientInstanceId, TaskId};

use super::{SharedRpcGateway, SignallingGateway};

impl SharedRpcGateway {
    /// Converts a completed Web upload into an opaque draft attachment handle.
    pub(crate) fn create_uploaded_file_reference(
        &self,
        client_instance_id: &ClientInstanceId,
        task_id: TaskId,
        path: String,
        label: String,
    ) -> Result<PreSendAttachment, ProtocolError> {
        self.lock_for_uploading_client(client_instance_id)?
            .attachments
            .create_uploaded_file_reference(client_instance_id, &task_id, path, label)
    }

    /// Registers a completed image upload as a pre-Send Image handle for this client and Task.
    pub(crate) fn create_uploaded_image(
        &self,
        client_instance_id: &ClientInstanceId,
        task_id: TaskId,
        path: String,
        label: String,
    ) -> Result<PreSendAttachment, ProtocolError> {
        self.lock_for_uploading_client(client_instance_id)?
            .attachments
            .create_uploaded_image(client_instance_id, &task_id, path, label)
    }

    /// An upload belongs to an initialized client; a stale one recovers by reconnecting.
    fn lock_for_uploading_client(
        &self,
        client_instance_id: &ClientInstanceId,
    ) -> Result<SignallingGateway<'_>, ProtocolError> {
        let gateway = self.lock();
        if gateway
            .client_hub
            .client_by_instance(client_instance_id)
            .is_none()
        {
            return Err(ProtocolError {
                code: ProtocolErrorCode::NotInitialized,
                message: "client connection is not initialized".to_string(),
                recoverable: true,
                target: None,
            });
        }
        Ok(gateway)
    }
}
