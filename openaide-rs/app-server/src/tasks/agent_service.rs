use crate::agent::gateway::AgentGateway;
use crate::agent::registry::AgentRegistry;
use crate::agent::status_cache::AgentStatusCache;
use crate::agent::{AgentAuthenticateRequest, AgentListSessionsRequest, AgentProbeRequest};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{AgentAuthenticateResult, AgentListSessionsResult, AgentProbeResult};
use crate::protocol::params::{AgentAuthenticateParams, AgentListSessionsParams, AgentProbeParams};

#[derive(Clone)]
pub(crate) struct AgentService {
    gateway: AgentGateway,
    registry: AgentRegistry,
    statuses: AgentStatusCache,
}

impl AgentService {
    pub(crate) fn with_status_cache(
        gateway: AgentGateway,
        registry: AgentRegistry,
        statuses: AgentStatusCache,
    ) -> Self {
        Self {
            gateway,
            registry,
            statuses,
        }
    }

    pub(crate) fn probe(&self, params: AgentProbeParams) -> Result<AgentProbeResult, RuntimeError> {
        self.registry.require(&params.agent_id)?;
        let agent_id = params.agent_id;
        let result = self.gateway.probe(AgentProbeRequest {
            agent_id: agent_id.clone(),
        });
        match &result {
            Ok(probe) => self.statuses.record_probe_success(probe),
            Err(error) => self.statuses.record_probe_error(&agent_id, error),
        }
        result
    }

    pub(crate) fn authenticate(
        &self,
        params: AgentAuthenticateParams,
    ) -> Result<AgentAuthenticateResult, RuntimeError> {
        self.registry.require(&params.agent_id)?;
        if params.method_id.trim().is_empty() {
            return Err(RuntimeError::InvalidParams("method_id".to_string()));
        }
        self.gateway.authenticate(AgentAuthenticateRequest {
            agent_id: params.agent_id,
            method_id: params.method_id,
        })
    }

    pub(crate) fn list_sessions(
        &self,
        params: AgentListSessionsParams,
    ) -> Result<AgentListSessionsResult, RuntimeError> {
        self.registry.require(&params.agent_id)?;
        let workspace_root = params.workspace_root.trim();
        if workspace_root.is_empty() {
            return Err(RuntimeError::InvalidParams("workspace_root".to_string()));
        }
        let workspace_root = std::path::Path::new(workspace_root);
        if !workspace_root.is_absolute() {
            return Err(RuntimeError::InvalidParams("workspace_root".to_string()));
        }
        self.gateway.list_sessions(AgentListSessionsRequest {
            agent_id: params.agent_id,
            cwd: workspace_root.to_string_lossy().to_string(),
            cursor: params.cursor,
        })
    }
}

#[cfg(test)]
mod tests;
