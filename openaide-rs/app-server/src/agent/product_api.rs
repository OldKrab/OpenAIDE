use std::sync::Arc;

use openaide_app_server_protocol::agent::{
    AgentAuthenticateParams as ProtocolAgentAuthenticateParams,
    AgentAuthenticateResult as ProtocolAgentAuthenticateResult,
    AgentAuthenticateStatus as ProtocolAgentAuthenticateStatus,
    AgentProbeParams as ProtocolAgentProbeParams, AgentProbeResult as ProtocolAgentProbeResult,
};
use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::snapshot::AgentCollectionSnapshot;

use crate::agent::catalog_store::AgentCatalogStore;
use crate::agent::gateway::AgentGateway;
use crate::agent::registry_handle::AgentRegistryHandle;
use crate::agent::status_cache::AgentStatusCache;
use crate::agent::{AgentAuthenticateRequest, AgentProbeRequest, AgentRuntime};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{AgentAuthenticateResult, AgentAuthenticateStatus, AgentProbeResult};
use crate::snapshots::{AgentCollectionSnapshotSource, AgentRegistrySnapshotSource};

mod catalog_mutations;
pub(crate) use catalog_mutations::AgentCatalogMutationWorkflow;
mod settings_details;
pub(crate) use settings_details::AgentSettingsDetailsWorkflow;

#[derive(Clone)]
pub(crate) struct AgentProductApi {
    pub(super) registry: AgentRegistryHandle,
    pub(super) catalog_store: AgentCatalogStore,
    gateway: AgentGateway,
    statuses: AgentStatusCache,
}

pub(crate) trait AgentProbeWorkflow: Send + Sync {
    fn probe(
        &self,
        params: ProtocolAgentProbeParams,
    ) -> Result<ProtocolAgentProbeResult, ProtocolError>;
}

pub(crate) trait AgentAuthenticateWorkflow: Send + Sync {
    fn authenticate(
        &self,
        params: ProtocolAgentAuthenticateParams,
    ) -> Result<ProtocolAgentAuthenticateResult, ProtocolError>;
}

impl AgentProductApi {
    pub(crate) fn new(
        registry: impl Into<AgentRegistryHandle>,
        catalog_store: AgentCatalogStore,
        runtime: Arc<dyn AgentRuntime>,
        statuses: AgentStatusCache,
    ) -> Self {
        Self {
            registry: registry.into(),
            catalog_store,
            gateway: AgentGateway::new(runtime),
            statuses,
        }
    }

    pub(super) fn snapshot(&self) -> Result<AgentCollectionSnapshot, ProtocolError> {
        AgentRegistrySnapshotSource::with_status_cache(self.registry.clone(), self.statuses.clone())
            .snapshot()
    }

    fn record_probe_result(
        &self,
        agent_id: &str,
        result: Result<AgentProbeResult, RuntimeError>,
    ) -> Result<(), ProtocolError> {
        match result {
            Ok(result) => {
                self.statuses.record_probe_success(&result);
                Ok(())
            }
            Err(error) => {
                self.statuses.record_probe_error(agent_id, &error);
                if expected_probe_status_error(&error) {
                    Ok(())
                } else {
                    Err(protocol_error_from_runtime(error))
                }
            }
        }
    }
}

impl AgentProbeWorkflow for AgentProductApi {
    fn probe(
        &self,
        params: ProtocolAgentProbeParams,
    ) -> Result<ProtocolAgentProbeResult, ProtocolError> {
        self.registry
            .require(params.agent_id.as_str())
            .map_err(protocol_error_from_runtime)?;
        let agent_id = params.agent_id.into_string();
        let probe = self.gateway.probe(AgentProbeRequest {
            agent_id: agent_id.clone(),
        });
        self.record_probe_result(&agent_id, probe)?;
        Ok(ProtocolAgentProbeResult {
            agents: self.snapshot()?,
        })
    }
}

impl AgentAuthenticateWorkflow for AgentProductApi {
    fn authenticate(
        &self,
        params: ProtocolAgentAuthenticateParams,
    ) -> Result<ProtocolAgentAuthenticateResult, ProtocolError> {
        self.registry
            .require(params.agent_id.as_str())
            .map_err(protocol_error_from_runtime)?;
        if params.method_id.trim().is_empty() {
            return Err(protocol_error_from_runtime(RuntimeError::InvalidParams(
                "method_id".to_string(),
            )));
        }
        let agent_id = params.agent_id.as_str().to_string();
        self.statuses
            .begin_authentication(&agent_id, &params.method_id, params.terminal_confirmed)
            .map_err(protocol_error_from_runtime)?;
        let result = self.gateway.authenticate(AgentAuthenticateRequest {
            agent_id: params.agent_id.as_str().to_string(),
            method_id: params.method_id.clone(),
            env: params.env.into_iter().collect(),
            secret_env: params.secret_env,
            secret_storage_agent_id: params.secret_storage_agent_id,
            terminal_confirmed: params.terminal_confirmed,
        });
        let result = match result {
            Ok(result) => {
                if matches!(result.status, AgentAuthenticateStatus::Authenticated) {
                    self.statuses.record_authentication_success(&agent_id);
                }
                result
            }
            Err(error) => {
                self.statuses.record_authentication_error(&agent_id, &error);
                return Err(protocol_authentication_error(error));
            }
        };
        Ok(protocol_authenticate_result(result))
    }
}

fn protocol_authenticate_result(
    result: AgentAuthenticateResult,
) -> ProtocolAgentAuthenticateResult {
    ProtocolAgentAuthenticateResult {
        agent_id: result.agent_id.into(),
        method_id: result.method_id,
        status: match result.status {
            AgentAuthenticateStatus::Authenticated => {
                ProtocolAgentAuthenticateStatus::Authenticated
            }
            AgentAuthenticateStatus::AwaitingUser => ProtocolAgentAuthenticateStatus::AwaitingUser,
        },
    }
}

/// Authentication errors cross a user-facing trust boundary; Agent details stay server-side.
fn protocol_authentication_error(error: RuntimeError) -> ProtocolError {
    let mut protocol_error = protocol_error_from_runtime(error);
    protocol_error.message =
        "Authentication failed. Check the Agent's requirements and try again.".to_string();
    protocol_error
}

fn expected_probe_status_error(error: &RuntimeError) -> bool {
    matches!(
        error,
        RuntimeError::CapabilityMissing(_)
            | RuntimeError::MethodNotFound(_)
            | RuntimeError::AuthRequired(_)
            | RuntimeError::SetupRequired(_)
            | RuntimeError::Unsupported(_)
    )
}

pub(super) fn protocol_error_from_runtime(error: RuntimeError) -> ProtocolError {
    match error {
        RuntimeError::CapabilityMissing(message) => ProtocolError {
            code: ProtocolErrorCode::CapabilityUnavailable,
            message,
            recoverable: true,
            target: None,
        },
        RuntimeError::AuthRequired(message)
        | RuntimeError::SetupRequired(message)
        | RuntimeError::Unsupported(message) => ProtocolError {
            code: ProtocolErrorCode::CapabilityUnavailable,
            message,
            recoverable: true,
            target: None,
        },
        RuntimeError::InvalidParams(field) => ProtocolError {
            code: ProtocolErrorCode::ValidationFailed,
            message: format!("Invalid field: {field}"),
            recoverable: false,
            target: None,
        },
        RuntimeError::TaskNotFound(message) => ProtocolError {
            code: ProtocolErrorCode::NotFound,
            message,
            recoverable: false,
            target: None,
        },
        other => ProtocolError {
            code: ProtocolErrorCode::Internal,
            message: other.to_string(),
            recoverable: true,
            target: None,
        },
    }
}

#[cfg(test)]
mod tests;
