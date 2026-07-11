use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;

use tokio::sync::mpsc as tokio_mpsc;

use crate::agent::acp_host_terminal_ownership::{AcpHostTerminalRegistry, AcpTerminalOwner};
use crate::agent::acp_runtime_threading::block_on_new_runtime;
use crate::agent::acp_session_client::record_terminal_error;
use crate::agent::acp_session_worker::{
    run_acp_agent_process, AcpAgentProcessInput, AcpAgentProcessOpen,
};
use crate::agent::registry_handle::AgentRegistryHandle;
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
    terminal_error: Arc<Mutex<Option<String>>>,
    terminal_registry: AcpHostTerminalRegistry,
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

        self.launch_process(agent_id, open)
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

    fn launch_process(
        &self,
        agent_id: &str,
        first_open: AcpAgentProcessOpen,
    ) -> Result<AcpAgentProcessSession, RuntimeError> {
        let config = self.registry.require_acp_config(agent_id)?;
        let host_bridge = self.host_bridge.clone();
        let terminal_registry = AcpHostTerminalRegistry::new(host_bridge.clone());
        let terminal_owner = terminal_registry.owner(first_open.terminal_owner_id);
        let (open_tx, open_rx) = tokio_mpsc::unbounded_channel();
        let terminal_error = Arc::new(Mutex::new(None));
        let process = AcpAgentProcessClient {
            open_tx: open_tx.clone(),
            terminal_error: terminal_error.clone(),
            terminal_registry: terminal_registry.clone(),
        };
        self.processes
            .lock()
            .expect("ACP process registry poisoned")
            .insert(agent_id.to_string(), process);

        let worker_agent_id = agent_id.to_string();
        let worker_terminal_error = terminal_error.clone();
        thread::spawn(move || {
            let result = block_on_new_runtime(run_acp_agent_process(AcpAgentProcessInput {
                config,
                first_open,
                open_rx,
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
        Ok(AcpAgentProcessSession {
            terminal_error,
            terminal_owner,
        })
    }
}
