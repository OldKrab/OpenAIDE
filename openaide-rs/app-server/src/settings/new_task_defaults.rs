use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::settings::NewTaskDefaultsUpdateParams;
use openaide_app_server_protocol::snapshot::NewTaskDefaultsSnapshot;

use crate::protocol::errors::RuntimeError;
use crate::storage::Store;

pub(crate) trait NewTaskDefaultsWorkflow: Send + Sync {
    fn update_defaults(
        &self,
        params: NewTaskDefaultsUpdateParams,
    ) -> Result<NewTaskDefaultsSnapshot, ProtocolError>;
}

#[derive(Clone)]
pub(crate) struct NewTaskDefaultsService {
    store: Store,
}

impl NewTaskDefaultsService {
    pub(crate) fn new(store: Store) -> Self {
        Self { store }
    }
}

impl NewTaskDefaultsWorkflow for NewTaskDefaultsService {
    fn update_defaults(
        &self,
        params: NewTaskDefaultsUpdateParams,
    ) -> Result<NewTaskDefaultsSnapshot, ProtocolError> {
        let mut defaults = self
            .store
            .read_new_task_defaults()
            .map_err(protocol_error_from_runtime)?;
        if let Some(project_id) = params.project_id {
            defaults.project_id = Some(project_id);
        }
        if let Some(agent_id) = params.agent_id {
            defaults.agent_id = Some(agent_id);
        }
        self.store
            .write_new_task_defaults(&defaults)
            .map_err(protocol_error_from_runtime)?;
        Ok(defaults)
    }
}

fn protocol_error_from_runtime(error: RuntimeError) -> ProtocolError {
    ProtocolError {
        code: ProtocolErrorCode::Internal,
        message: error.to_string(),
        recoverable: true,
        target: None,
    }
}

#[cfg(test)]
#[path = "new_task_defaults_tests.rs"]
mod tests;
