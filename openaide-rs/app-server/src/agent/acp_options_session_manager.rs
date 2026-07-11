use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Mutex};
use std::thread;
use std::time::Duration;

use crate::agent::acp_auth_method_cache::AcpAuthMethodCache;
use crate::agent::acp_options_session::{run_options_session, AcpOptionsSessionWorkerInput};
use crate::agent::acp_options_session_client::{options_session_channel, AcpOptionsSessionClient};
use crate::agent::acp_runtime_threading::block_on_new_runtime;
use crate::agent::acp_session_paths::normalized_session_cwd;
use crate::agent::registry_handle::AgentRegistryHandle;
use crate::logging;
use crate::protocol::errors::RuntimeError;
use crate::protocol::host::HostBridge;

use crate::agent::acp_errors::startup_error_message;

const START_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone)]
struct AcpOptionsSessionEntry {
    cwd: PathBuf,
    request_key: String,
    client: ManagedOptionsSessionClient,
}

#[derive(Clone)]
struct ManagedOptionsSessionClient {
    client: AcpOptionsSessionClient,
    generation: u64,
}

impl ManagedOptionsSessionClient {
    fn close(self) -> Result<(), RuntimeError> {
        self.client.close()
    }
}

pub(super) struct AcpOptionsSessionManager {
    registry: AgentRegistryHandle,
    host_bridge: HostBridge,
    active: Mutex<Option<AcpOptionsSessionEntry>>,
    auth_method_cache: AcpAuthMethodCache,
    next_generation: AtomicU64,
}

impl AcpOptionsSessionManager {
    pub(super) fn new(
        registry: impl Into<AgentRegistryHandle>,
        host_bridge: HostBridge,
        auth_method_cache: AcpAuthMethodCache,
    ) -> Self {
        Self {
            registry: registry.into(),
            host_bridge,
            active: Mutex::new(None),
            auth_method_cache,
            next_generation: AtomicU64::new(0),
        }
    }

    pub(super) fn take_shutdown_close_task(&self) -> Option<Box<dyn FnOnce() + Send + 'static>> {
        self.active
            .lock()
            .expect("ACP options session registry poisoned")
            .take()
            .map(|entry| {
                Box::new(move || {
                    let _ = entry.client.close();
                }) as Box<dyn FnOnce() + Send + 'static>
            })
    }

    pub(super) fn with_options_session<T>(
        &self,
        agent_id: &str,
        cwd: &str,
        operation: impl Fn(&AcpOptionsSessionClient) -> Result<T, RuntimeError>,
    ) -> Result<T, RuntimeError> {
        let client = self.ensure_options_session(agent_id, cwd)?;
        match operation(&client.client) {
            Ok(result) => Ok(result),
            Err(RuntimeError::NotReady(_)) => {
                self.invalidate_options_session(client.generation);
                let retry_client = self.ensure_options_session(agent_id, cwd)?;
                let retry = operation(&retry_client.client);
                if matches!(retry, Err(RuntimeError::NotReady(_))) {
                    self.invalidate_options_session(retry_client.generation);
                }
                retry
            }
            Err(error) => Err(error),
        }
    }

    fn ensure_options_session(
        &self,
        agent_id: &str,
        cwd: &str,
    ) -> Result<ManagedOptionsSessionClient, RuntimeError> {
        let registry = self.registry.current();
        let agent = registry.require(agent_id)?;
        let request_key = agent.options_request_key(cwd);
        let cwd = normalized_session_cwd(cwd);
        let mut active = self
            .active
            .lock()
            .expect("ACP options session registry poisoned");
        if let Some(entry) = active.as_ref() {
            if entry.cwd == cwd && entry.request_key == request_key {
                return Ok(entry.client.clone());
            }
        }
        if let Some(entry) = active.take() {
            let _ = entry.client.close();
        }

        let (options_client, command_rx) = options_session_channel();
        let (started_tx, started_rx) = mpsc::channel();
        let worker_started_tx = started_tx.clone();
        let config = agent.acp_stdio_config();
        let host_bridge = self.host_bridge.clone();
        let worker_agent_id = agent_id.to_string();
        let worker_cwd = cwd.clone();
        let auth_method_id = self.auth_method_cache.preferred_method();
        let generation = self.next_generation.fetch_add(1, Ordering::SeqCst) + 1;

        thread::spawn(move || {
            let result = block_on_new_runtime(run_options_session(AcpOptionsSessionWorkerInput {
                config,
                agent_id: worker_agent_id,
                cwd: worker_cwd,
                auth_method_id,
                command_rx,
                started_tx,
                host_bridge,
            }))
            .and_then(|result| result);
            if let Err(error) = result {
                let _ = worker_started_tx.send(Err(startup_error_message(&error)));
                logging::warn(
                    "acp_options_session_ended",
                    serde_json::json!({ "error": error.to_string() }),
                );
                eprintln!("OpenAIDE ACP options session ended: {error}");
            } else {
                let _ = worker_started_tx.send(Err(
                    "ACP options session ended before startup completed".to_string(),
                ));
            }
        });

        match started_rx.recv_timeout(START_TIMEOUT) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let _ = options_client.close();
                Err("ACP options session start timed out".to_string())
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err("ACP options session ended before startup completed".to_string())
            }
        }
        .map_err(RuntimeError::NotReady)?;

        let client = ManagedOptionsSessionClient {
            client: options_client,
            generation,
        };
        *active = Some(AcpOptionsSessionEntry {
            cwd,
            request_key,
            client: client.clone(),
        });
        Ok(client)
    }

    fn invalidate_options_session(&self, generation: u64) {
        let entry = {
            let mut active = self
                .active
                .lock()
                .expect("ACP options session registry poisoned");
            if active
                .as_ref()
                .is_some_and(|entry| entry.client.generation == generation)
            {
                active.take()
            } else {
                None
            }
        };
        if let Some(entry) = entry {
            let _ = entry.client.close();
        }
    }
}
