use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::settings::{
    AppPreferencesParams, AppPreferencesResult, AppPreferencesUpdateParams,
};

use crate::protocol::errors::RuntimeError;
use crate::storage::Store;

pub(crate) trait AppPreferencesWorkflow: Send + Sync {
    fn app_preferences(
        &self,
        params: AppPreferencesParams,
    ) -> Result<AppPreferencesResult, ProtocolError>;
    fn update_app_preferences(
        &self,
        params: AppPreferencesUpdateParams,
    ) -> Result<AppPreferencesResult, ProtocolError>;
}

#[derive(Clone)]
pub(crate) struct AppPreferencesService {
    store: Store,
}

impl AppPreferencesService {
    pub(crate) fn new(store: Store) -> Self {
        Self { store }
    }
}

impl AppPreferencesWorkflow for AppPreferencesService {
    fn app_preferences(
        &self,
        _params: AppPreferencesParams,
    ) -> Result<AppPreferencesResult, ProtocolError> {
        Ok(AppPreferencesResult {
            preferences: self
                .store
                .read_app_preferences()
                .map_err(protocol_error_from_runtime)?,
        })
    }

    fn update_app_preferences(
        &self,
        params: AppPreferencesUpdateParams,
    ) -> Result<AppPreferencesResult, ProtocolError> {
        Ok(AppPreferencesResult {
            preferences: self
                .store
                .update_app_preferences(params.preferences)
                .map_err(protocol_error_from_runtime)?,
        })
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
