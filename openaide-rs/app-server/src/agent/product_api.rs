use std::sync::Arc;

use openaide_app_server_protocol::agent::{
    AgentAuthenticateParams as ProtocolAgentAuthenticateParams,
    AgentAuthenticateResult as ProtocolAgentAuthenticateResult,
    AgentAuthenticateStatus as ProtocolAgentAuthenticateStatus,
    AgentCancelAuthenticateParams as ProtocolAgentCancelAuthenticateParams,
    AgentCancelAuthenticateResult as ProtocolAgentCancelAuthenticateResult,
    AgentLogoutParams as ProtocolAgentLogoutParams, AgentLogoutResult as ProtocolAgentLogoutResult,
    AgentProbeParams as ProtocolAgentProbeParams, AgentProbeResult as ProtocolAgentProbeResult,
};
use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::snapshot::{AgentCollectionSnapshot, AgentStatus};

use crate::agent::auth_provenance_store::AgentAuthProvenanceStore;
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
    store: crate::storage::Store,
    auth_provenance: AgentAuthProvenanceStore,
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

    fn authenticate_with_secret_resolver(
        &self,
        params: ProtocolAgentAuthenticateParams,
        secret_resolver: Option<Arc<dyn crate::agent::AgentSecretResolver>>,
    ) -> Result<ProtocolAgentAuthenticateResult, ProtocolError> {
        let _ = secret_resolver;
        self.authenticate(params)
    }

    fn cancel_authenticate(
        &self,
        params: ProtocolAgentCancelAuthenticateParams,
    ) -> Result<ProtocolAgentCancelAuthenticateResult, ProtocolError> {
        let _ = params;
        Err(ProtocolError {
            code: ProtocolErrorCode::CapabilityUnavailable,
            message: "Authentication cancel is unavailable.".to_string(),
            recoverable: true,
            target: None,
        })
    }

    fn logout(
        &self,
        params: ProtocolAgentLogoutParams,
    ) -> Result<ProtocolAgentLogoutResult, ProtocolError> {
        let _ = params;
        Err(ProtocolError {
            code: ProtocolErrorCode::CapabilityUnavailable,
            message: "Sign out is unavailable.".to_string(),
            recoverable: true,
            target: None,
        })
    }

    /// The Agent asked the user to open `url` during the running Sign-in Flow. Returns whether a
    /// flow accepted it; the ACP URL elicitation is answered accordingly.
    fn record_sign_in_url(&self, agent_id: &str, url: String, hint: Option<String>) -> bool {
        let _ = (agent_id, url, hint);
        false
    }
}

impl AgentProductApi {
    pub(crate) fn new(
        registry: impl Into<AgentRegistryHandle>,
        catalog_store: AgentCatalogStore,
        runtime: Arc<dyn AgentRuntime>,
        statuses: AgentStatusCache,
    ) -> Self {
        let store = catalog_store.backing_store();
        let auth_provenance = AgentAuthProvenanceStore::new(store.clone());
        Self {
            registry: registry.into(),
            catalog_store,
            gateway: AgentGateway::new(runtime),
            statuses,
            store,
            auth_provenance,
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
        self.authenticate_with_secret_resolver(params, None)
    }

    fn authenticate_with_secret_resolver(
        &self,
        params: ProtocolAgentAuthenticateParams,
        secret_resolver: Option<Arc<dyn crate::agent::AgentSecretResolver>>,
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
        // Protocol-boundary lifecycle record; the ACP exchange itself is logged by the runtime.
        let started_at = std::time::Instant::now();
        crate::logging::info(
            "agent_authenticate_started",
            serde_json::json!({
                "agent_id": agent_id,
                "method_id": params.method_id,
                "terminal_confirmed": params.terminal_confirmed,
                "has_env": !params.env.is_empty() || !params.secret_env.is_empty(),
            }),
        );
        let result = self.gateway.authenticate(AgentAuthenticateRequest {
            agent_id: params.agent_id.as_str().to_string(),
            method_id: params.method_id.clone(),
            env: params.env.into_iter().collect(),
            secret_env: params.secret_env,
            secret_storage_agent_id: params.secret_storage_agent_id,
            terminal_confirmed: params.terminal_confirmed,
            secret_resolver,
        });
        let result = match result {
            Ok(result) => {
                crate::logging::info(
                    "agent_authenticate_completed",
                    serde_json::json!({
                        "agent_id": agent_id,
                        "method_id": params.method_id,
                        "status": result.status,
                        "duration_ms": started_at.elapsed().as_millis(),
                    }),
                );
                match result.status {
                    AgentAuthenticateStatus::Authenticated => {
                        if let Err(error) =
                            self.auth_provenance.record(&agent_id, &params.method_id)
                        {
                            // A staged App Shell secret will roll back when this request fails.
                            // Stop the process so it cannot retain an environment we failed to own.
                            let _ = self.gateway.cancel_authentication(&agent_id);
                            self.statuses
                                .record_authentication_error(&agent_id, &error, None);
                            return Err(protocol_error_from_runtime(error));
                        }
                        self.statuses.record_authentication_success(&agent_id);
                    }
                    AgentAuthenticateStatus::AwaitingUser => {
                        self.statuses.record_sign_in_awaiting_terminal(&agent_id);
                    }
                }
                result
            }
            Err(error) => {
                crate::logging::warn(
                    "agent_authenticate_failed",
                    serde_json::json!({
                        "agent_id": agent_id,
                        "method_id": params.method_id,
                        "duration_ms": started_at.elapsed().as_millis(),
                        "error_kind": error.reason(),
                    }),
                );
                let cancelled =
                    self.statuses.snapshot(&agent_id).status != AgentStatus::Authenticating;
                let failure = self.authentication_failure_message(&agent_id, &params.method_id);
                // Cancel already rolled the status back; a cancelled flow leaves no failure.
                self.statuses
                    .record_authentication_error(&agent_id, &error, Some(failure.clone()));
                let mut protocol_error = protocol_error_from_runtime(error);
                protocol_error.message = if cancelled {
                    "Sign-in cancelled.".to_string()
                } else {
                    failure
                };
                return Err(protocol_error);
            }
        };
        Ok(protocol_authenticate_result(result, self.snapshot()?))
    }

    fn cancel_authenticate(
        &self,
        params: ProtocolAgentCancelAuthenticateParams,
    ) -> Result<ProtocolAgentCancelAuthenticateResult, ProtocolError> {
        self.registry
            .require(params.agent_id.as_str())
            .map_err(protocol_error_from_runtime)?;
        let agent_id = params.agent_id.as_str();
        let cancel_result = self.gateway.cancel_authentication(agent_id);
        crate::logging::info(
            "agent_cancel_authenticate",
            serde_json::json!({
                "agent_id": agent_id,
                "was_authenticating": self.statuses.snapshot(agent_id).status == AgentStatus::Authenticating,
                "runtime_outcome": match &cancel_result {
                    Ok(()) => "ok",
                    Err(error) => error.reason(),
                },
            }),
        );
        if self.statuses.snapshot(agent_id).status == AgentStatus::Authenticating {
            self.statuses.record_authentication_error(
                agent_id,
                &RuntimeError::NotReady("Sign-in cancelled.".to_string()),
                None,
            );
        } else {
            // Cancel on a finished flow acknowledges its failure.
            self.statuses.dismiss_failed_sign_in(agent_id);
        }
        Ok(ProtocolAgentCancelAuthenticateResult {
            agents: self.snapshot()?,
        })
    }

    fn logout(
        &self,
        params: ProtocolAgentLogoutParams,
    ) -> Result<ProtocolAgentLogoutResult, ProtocolError> {
        let agent_id = params.agent_id.as_str();
        self.registry
            .require(agent_id)
            .map_err(protocol_error_from_runtime)?;
        if !self.statuses.snapshot(agent_id).logout_supported {
            return Err(ProtocolError {
                code: ProtocolErrorCode::CapabilityUnavailable,
                message: "This Agent does not support sign out.".to_string(),
                recoverable: true,
                target: None,
            });
        }
        let recorded_method_id = self
            .auth_provenance
            .method(agent_id)
            .map_err(protocol_error_from_runtime)?;
        if params.expected_method_id != recorded_method_id {
            return Err(ProtocolError {
                code: ProtocolErrorCode::Conflict,
                message: "Authentication settings changed. Refresh Agent settings and try again."
                    .to_string(),
                recoverable: true,
                target: None,
            });
        }
        if self
            .has_running_task(agent_id)
            .map_err(protocol_error_from_runtime)?
        {
            return Err(ProtocolError {
                code: ProtocolErrorCode::Conflict,
                message: "Stop this Agent's running Tasks before signing out.".to_string(),
                recoverable: true,
                target: None,
            });
        }
        let started_at = std::time::Instant::now();
        crate::logging::info(
            "agent_logout_started",
            serde_json::json!({ "agent_id": agent_id }),
        );
        if let Err(error) = self.gateway.logout(agent_id) {
            crate::logging::warn(
                "agent_logout_failed",
                serde_json::json!({
                    "agent_id": agent_id,
                    "duration_ms": started_at.elapsed().as_millis(),
                    "error_kind": error.reason(),
                }),
            );
            return Err(protocol_error_from_runtime(error));
        }
        // ACP logout cannot be rolled back. Keep the successful result so the initiating App
        // Shell commits credential deletion even if this non-secret cleanup marker cannot update.
        if let Err(error) = self.auth_provenance.clear(agent_id) {
            crate::logging::warn(
                "agent_logout_provenance_cleanup_failed",
                serde_json::json!({
                    "agent_id": agent_id,
                    "error_kind": error.reason(),
                }),
            );
        }
        self.statuses.record_logout_success(agent_id);
        crate::logging::info(
            "agent_logout_completed",
            serde_json::json!({
                "agent_id": agent_id,
                "duration_ms": started_at.elapsed().as_millis(),
            }),
        );
        Ok(ProtocolAgentLogoutResult {
            agents: self.snapshot()?,
        })
    }

    fn record_sign_in_url(&self, agent_id: &str, url: String, hint: Option<String>) -> bool {
        let recorded = self
            .statuses
            .record_sign_in_awaiting_user(agent_id, url, hint);
        crate::logging::info(
            "agent_sign_in_url_recorded",
            serde_json::json!({ "agent_id": agent_id, "accepted": recorded }),
        );
        recorded
    }
}

impl AgentProductApi {
    pub(super) fn has_running_task(&self, agent_id: &str) -> Result<bool, RuntimeError> {
        Ok(self
            .store
            .list_all_task_records_strict()?
            .into_iter()
            .any(|task| {
                task.agent_id == agent_id
                    && !task.tombstoned
                    && matches!(
                        task.status,
                        crate::protocol::model::TaskStatus::Starting
                            | crate::protocol::model::TaskStatus::Active
                            | crate::protocol::model::TaskStatus::Stopping
                            | crate::protocol::model::TaskStatus::Waiting
                    )
            }))
    }
}

impl AgentProductApi {
    /// Authentication errors cross a user-facing trust boundary; Agent details stay server-side.
    fn authentication_failure_message(&self, agent_id: &str, method_id: &str) -> String {
        let agent_label = self
            .registry
            .display_name(agent_id, None)
            .unwrap_or_else(|_| agent_id.to_string());
        let method_label = self
            .statuses
            .snapshot(agent_id)
            .auth_methods
            .into_iter()
            .find(|method| method.id == method_id)
            .map(|method| method.label)
            .unwrap_or_else(|| method_id.to_string());
        format!("{agent_label} could not sign in with {method_label}. Try again or choose another method.")
    }
}

fn protocol_authenticate_result(
    result: AgentAuthenticateResult,
    agents: AgentCollectionSnapshot,
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
        agents,
    }
}

fn expected_probe_status_error(error: &RuntimeError) -> bool {
    matches!(
        error,
        RuntimeError::CapabilityMissing(_)
            | RuntimeError::MethodNotFound(_)
            | RuntimeError::AuthRequired(_)
            | RuntimeError::SetupRequired(_)
            | RuntimeError::NodeJsRequired(_)
            | RuntimeError::NotReady(_)
            | RuntimeError::Internal(_)
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
        RuntimeError::AuthRequired(message) => ProtocolError {
            code: ProtocolErrorCode::Unauthorized,
            message,
            recoverable: true,
            target: None,
        },
        RuntimeError::NodeJsRequired(message) => ProtocolError {
            code: ProtocolErrorCode::NodeJsRequired,
            message,
            recoverable: true,
            target: None,
        },
        RuntimeError::SetupRequired(message) | RuntimeError::Unsupported(message) => {
            ProtocolError {
                code: ProtocolErrorCode::CapabilityUnavailable,
                message,
                recoverable: true,
                target: None,
            }
        }
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
