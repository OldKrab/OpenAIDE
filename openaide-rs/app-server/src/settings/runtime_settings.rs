use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::settings::{
    RuntimeAcpTraceSettings, RuntimeDeveloperSettings, RuntimeSettingsResult,
    RuntimeSettingsUpdateParams,
};

use crate::agent::acp_trace::{AcpTraceState, AcpTraceStatus};
use crate::protocol::errors::RuntimeError;

pub(crate) trait RuntimeSettingsWorkflow: Send + Sync {
    fn runtime_settings(&self) -> Result<RuntimeSettingsResult, ProtocolError>;
    fn update_runtime_settings(
        &self,
        params: RuntimeSettingsUpdateParams,
    ) -> Result<RuntimeSettingsResult, ProtocolError>;
}

#[derive(Debug, Clone)]
pub(crate) struct RuntimeSettingsService {
    acp_trace_state: AcpTraceState,
}

impl RuntimeSettingsService {
    pub(crate) fn new(acp_trace_state: AcpTraceState) -> Self {
        Self { acp_trace_state }
    }

    fn current(&self) -> RuntimeSettingsResult {
        protocol_settings(self.acp_trace_state.status())
    }
}

impl RuntimeSettingsWorkflow for RuntimeSettingsService {
    fn runtime_settings(&self) -> Result<RuntimeSettingsResult, ProtocolError> {
        Ok(self.current())
    }

    fn update_runtime_settings(
        &self,
        params: RuntimeSettingsUpdateParams,
    ) -> Result<RuntimeSettingsResult, ProtocolError> {
        if let Some(enabled) = params.developer.acp_trace.enabled {
            self.acp_trace_state
                .set_enabled(enabled)
                .map_err(protocol_error_from_runtime)?;
        }
        Ok(self.current())
    }
}

fn protocol_settings(status: AcpTraceStatus) -> RuntimeSettingsResult {
    RuntimeSettingsResult {
        developer: RuntimeDeveloperSettings {
            acp_trace: RuntimeAcpTraceSettings {
                enabled: status.enabled,
                directory: status.directory,
            },
        },
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
