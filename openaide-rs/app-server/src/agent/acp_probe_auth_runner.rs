use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::agent::acp_auth_method_cache::AcpAuthMethodCache;
use crate::agent::acp_probe_auth::{run_agent_authenticate, run_agent_probe};
use crate::agent::acp_runtime_threading::block_on_new_runtime;
use crate::agent::registry_handle::AgentRegistryHandle;
use crate::agent::{AgentAuthenticateRequest, AgentProbeRequest};
use crate::protocol::errors::RuntimeError;
use crate::protocol::host::HostBridge;
use crate::protocol::model::{AgentAuthenticateResult, AgentProbeResult};

pub(super) struct AcpProbeAuthRunner {
    registry: AgentRegistryHandle,
    host_bridge: HostBridge,
    auth_method_cache: AcpAuthMethodCache,
}

impl AcpProbeAuthRunner {
    pub(super) fn new(
        registry: impl Into<AgentRegistryHandle>,
        host_bridge: HostBridge,
        auth_method_cache: AcpAuthMethodCache,
    ) -> Self {
        Self {
            registry: registry.into(),
            host_bridge,
            auth_method_cache,
        }
    }

    pub(super) fn probe_with_timeout(
        &self,
        request: AgentProbeRequest,
        timeout: Duration,
    ) -> Result<AgentProbeResult, RuntimeError> {
        let (probe_tx, probe_rx) = mpsc::channel();
        let config = self.registry.require_acp_config(&request.agent_id)?;
        let host_bridge = self.host_bridge.clone();
        let agent_id = request.agent_id;
        thread::spawn(move || {
            let result =
                block_on_new_runtime(run_agent_probe(config, agent_id, timeout, host_bridge))
                    .and_then(|result| result);
            let _ = probe_tx.send(result);
        });

        probe_rx
            .recv_timeout(timeout + Duration::from_secs(1))
            .map_err(|error| {
                RuntimeError::NotReady(format!("ACP Agent probe timed out: {error}"))
            })?
    }

    pub(super) fn authenticate_with_timeout(
        &self,
        request: AgentAuthenticateRequest,
        timeout: Duration,
    ) -> Result<AgentAuthenticateResult, RuntimeError> {
        if request.method_id.trim().is_empty() {
            return Err(RuntimeError::InvalidParams("method_id".to_string()));
        }

        let (auth_tx, auth_rx) = mpsc::channel();
        let config = self.registry.require_acp_config(&request.agent_id)?;
        let host_bridge = self.host_bridge.clone();
        thread::spawn(move || {
            let result = block_on_new_runtime(run_agent_authenticate(
                config,
                request,
                timeout,
                host_bridge,
            ))
            .and_then(|result| result);
            let _ = auth_tx.send(result);
        });

        let result = auth_rx
            .recv_timeout(timeout + Duration::from_secs(1))
            .map_err(|error| {
                RuntimeError::NotReady(format!("ACP Agent authentication timed out: {error}"))
            })??;
        self.auth_method_cache
            .record_authenticated_method(result.method_id.clone());
        Ok(result)
    }
}
