use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;

use tokio::sync::mpsc as tokio_mpsc;

use crate::agent::acp_host_terminal_ownership::{AcpHostTerminalRegistry, AcpTerminalOwner};
use crate::agent::acp_runtime_threading::block_on_new_runtime;
use crate::agent::acp_session_client::record_terminal_error;
use crate::agent::acp_session_worker::{
    run_acp_agent_process, AcpAgentProcessControl, AcpAgentProcessInput, AcpAgentProcessList,
    AcpAgentProcessOpen,
};
use crate::agent::registry_handle::AgentRegistryHandle;
use crate::agent::{AgentAuthenticateRequest, AgentListSessionsRequest};
use crate::logging;
use crate::protocol::errors::RuntimeError;
use crate::protocol::host::HostBridge;

pub(super) struct AcpAgentProcessPool {
    registry: AgentRegistryHandle,
    host_bridge: HostBridge,
    processes: Mutex<HashMap<String, AcpAgentProcessClient>>,
}

#[derive(Clone)]
struct AcpAgentProcessClient {
    open_tx: tokio_mpsc::UnboundedSender<AcpAgentProcessOpen>,
    list_tx: tokio_mpsc::UnboundedSender<AcpAgentProcessList>,
    control_tx: tokio_mpsc::UnboundedSender<AcpAgentProcessControl>,
    terminal_error: Arc<Mutex<Option<String>>>,
    terminal_registry: AcpHostTerminalRegistry,
    shutdown_tx: tokio::sync::watch::Sender<bool>,
}

pub(super) struct AcpAgentProcessSession {
    pub(super) terminal_error: Arc<Mutex<Option<String>>>,
    pub(super) terminal_owner: AcpTerminalOwner,
}

impl AcpAgentProcessPool {
    pub(super) fn new(registry: AgentRegistryHandle, host_bridge: HostBridge) -> Self {
        Self {
            registry,
            host_bridge,
            processes: Mutex::new(HashMap::new()),
        }
    }

    pub(super) fn open_session(
        &self,
        agent_id: &str,
        mut open: AcpAgentProcessOpen,
    ) -> Result<AcpAgentProcessSession, RuntimeError> {
        if let Some(process) = self.existing_process(agent_id) {
            let owner_id = open.terminal_owner_id;
            match process.open_tx.send(open) {
                Ok(()) => {
                    return Ok(AcpAgentProcessSession {
                        terminal_error: process.terminal_error.clone(),
                        terminal_owner: process.terminal_registry.owner(owner_id),
                    });
                }
                Err(error) => {
                    open = error.0;
                }
            }
            self.remove_process(agent_id);
        }

        let owner_id = open.terminal_owner_id;
        let process = self.launch_process(agent_id, Some(open))?;
        Ok(AcpAgentProcessSession {
            terminal_error: process.terminal_error.clone(),
            terminal_owner: process.terminal_registry.owner(owner_id),
        })
    }

    pub(super) fn list_sessions(
        &self,
        request: AgentListSessionsRequest,
        preferred_auth_method_id: Option<String>,
    ) -> Result<crate::protocol::model::AgentListSessionsResult, RuntimeError> {
        let process = self.get_or_launch_process(&request.agent_id)?;
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        if process
            .list_tx
            .send(AcpAgentProcessList {
                request,
                preferred_auth_method_id,
                reply_tx,
            })
            .is_err()
        {
            return Err(RuntimeError::NotReady(
                "ACP agent process ended before session listing".to_string(),
            ));
        }
        reply_rx
            .recv_timeout(std::time::Duration::from_secs(30))
            .map_err(|_| RuntimeError::NotReady("ACP session listing timed out".to_string()))?
    }

    pub(super) fn probe(
        &self,
        agent_id: &str,
        timeout: std::time::Duration,
    ) -> Result<crate::protocol::model::AgentProbeResult, RuntimeError> {
        let process = self.get_or_launch_process(agent_id)?;
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        process
            .control_tx
            .send(AcpAgentProcessControl::Probe {
                agent_id: agent_id.to_string(),
                reply_tx,
            })
            .map_err(|_| {
                RuntimeError::NotReady("ACP agent process ended before probe".to_string())
            })?;
        match reply_rx.recv_timeout(timeout) {
            Ok(result) => result,
            Err(_) => {
                self.stop_process(agent_id, &process);
                Err(RuntimeError::NotReady(
                    "ACP Agent probe timed out".to_string(),
                ))
            }
        }
    }

    pub(super) fn authenticate(
        &self,
        request: AgentAuthenticateRequest,
    ) -> Result<crate::protocol::model::AgentAuthenticateResult, RuntimeError> {
        let process = self.get_or_launch_process(&request.agent_id)?;
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        process
            .control_tx
            .send(AcpAgentProcessControl::Authenticate { request, reply_tx })
            .map_err(|_| {
                RuntimeError::NotReady("ACP agent process ended before authentication".to_string())
            })?;
        reply_rx
            .recv_timeout(std::time::Duration::from_secs(120))
            .map_err(|_| RuntimeError::NotReady("ACP authentication timed out".to_string()))?
    }

    fn existing_process(&self, agent_id: &str) -> Option<AcpAgentProcessClient> {
        self.processes
            .lock()
            .expect("ACP process registry poisoned")
            .get(agent_id)
            .cloned()
    }

    fn remove_process(&self, agent_id: &str) {
        self.processes
            .lock()
            .expect("ACP process registry poisoned")
            .remove(agent_id);
    }

    fn stop_process(&self, agent_id: &str, process: &AcpAgentProcessClient) {
        self.remove_process(agent_id);
        let _ = process.shutdown_tx.send(true);
    }

    fn get_or_launch_process(&self, agent_id: &str) -> Result<AcpAgentProcessClient, RuntimeError> {
        match self.existing_process(agent_id) {
            Some(process) => Ok(process),
            None => self.launch_process(agent_id, None),
        }
    }

    fn launch_process(
        &self,
        agent_id: &str,
        first_open: Option<AcpAgentProcessOpen>,
    ) -> Result<AcpAgentProcessClient, RuntimeError> {
        let config = self.registry.require_acp_config(agent_id)?;
        config.ensure_command_available()?;
        let host_bridge = self.host_bridge.clone();
        let terminal_registry = AcpHostTerminalRegistry::new(host_bridge.clone());
        let (open_tx, open_rx) = tokio_mpsc::unbounded_channel();
        let (list_tx, list_rx) = tokio_mpsc::unbounded_channel();
        let (control_tx, control_rx) = tokio_mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let terminal_error = Arc::new(Mutex::new(None));
        let process = AcpAgentProcessClient {
            open_tx: open_tx.clone(),
            list_tx: list_tx.clone(),
            control_tx: control_tx.clone(),
            terminal_error: terminal_error.clone(),
            terminal_registry: terminal_registry.clone(),
            shutdown_tx,
        };
        self.processes
            .lock()
            .expect("ACP process registry poisoned")
            .insert(agent_id.to_string(), process.clone());

        let worker_agent_id = agent_id.to_string();
        let worker_terminal_error = terminal_error.clone();
        thread::spawn(move || {
            let result = block_on_new_runtime(run_acp_agent_process(AcpAgentProcessInput {
                config,
                first_open,
                open_rx,
                list_rx,
                control_rx,
                shutdown_rx,
                host_bridge,
                terminal_registry,
            }))
            .and_then(|result| result);
            if let Err(error) = result {
                record_terminal_error(&worker_terminal_error, &error);
                logging::warn(
                    "acp_agent_process_ended",
                    serde_json::json!({
                        "agentId": worker_agent_id,
                        "error": error.to_string(),
                    }),
                );
                eprintln!("OpenAIDE ACP agent process ended: {error}");
            }
        });
        drop(open_tx);
        drop(list_tx);
        drop(control_tx);
        Ok(process)
    }
}
