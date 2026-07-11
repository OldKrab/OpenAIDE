use std::env;
use std::path::Path;

use agent_client_protocol::AcpAgent;

use crate::agent::acp_trace::AcpTraceSession;
use crate::agent::AgentSecretResolver;
use crate::protocol::errors::RuntimeError;
use crate::protocol::host::HostBridge;

#[derive(Debug, Clone)]
pub struct AcpAgentConfig {
    pub agent_id: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub secret_env: Vec<String>,
}

impl AcpAgentConfig {
    pub fn codex() -> Self {
        if command_in_path("codex-acp") {
            Self {
                agent_id: "codex".to_string(),
                command: "codex-acp".to_string(),
                args: Vec::new(),
                env: Vec::new(),
                secret_env: Vec::new(),
            }
        } else {
            Self {
                agent_id: "codex".to_string(),
                command: "npx".to_string(),
                args: vec![
                    "-y".to_string(),
                    "@agentclientprotocol/codex-acp".to_string(),
                ],
                env: Vec::new(),
                secret_env: Vec::new(),
            }
        }
    }

    pub fn opencode() -> Self {
        if command_in_path("opencode") {
            Self {
                agent_id: "opencode".to_string(),
                command: "opencode".to_string(),
                args: vec!["acp".to_string()],
                env: Vec::new(),
                secret_env: Vec::new(),
            }
        } else {
            Self {
                agent_id: "opencode".to_string(),
                command: "npx".to_string(),
                args: vec![
                    "-y".to_string(),
                    "opencode-ai".to_string(),
                    "acp".to_string(),
                ],
                env: Vec::new(),
                secret_env: Vec::new(),
            }
        }
    }

    pub(crate) fn to_acp_agent(
        &self,
        trace: Option<AcpTraceSession>,
        host_bridge: &HostBridge,
        secret_resolver: Option<&dyn AgentSecretResolver>,
    ) -> Result<AcpAgent, RuntimeError> {
        self.ensure_command_available()?;
        let mut env = self.env.clone();
        env.extend(self.secret_env_values(host_bridge, secret_resolver)?);
        let args = env
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .chain(std::iter::once(self.command.clone()))
            .chain(self.args.clone())
            .collect::<Vec<_>>();
        let agent = AcpAgent::from_args(args).map_err(super::acp_errors::acp_error)?;
        Ok(match trace {
            Some(trace) => agent.with_debug(move |line, direction| {
                trace.record_line(line, direction);
            }),
            None => agent,
        })
    }

    fn ensure_command_available(&self) -> Result<(), RuntimeError> {
        if command_has_path_separator(&self.command) {
            let path = Path::new(&self.command);
            if !path.is_file() {
                return Err(command_not_found_error(&self.command));
            }
            return Ok(());
        }
        if command_in_path(&self.command) {
            Ok(())
        } else {
            Err(command_not_found_error(&self.command))
        }
    }

    fn secret_env_values(
        &self,
        host_bridge: &HostBridge,
        secret_resolver: Option<&dyn AgentSecretResolver>,
    ) -> Result<Vec<(String, String)>, RuntimeError> {
        if self.secret_env.is_empty() {
            return Ok(Vec::new());
        }
        let resolved = match secret_resolver {
            Some(resolver) => resolver.resolve_secret_env(&self.agent_id, &self.secret_env)?,
            None => legacy_host_secret_env(host_bridge, &self.agent_id, &self.secret_env)?,
        };
        self.secret_env
            .iter()
            .map(|name| {
                resolved
                    .get(name)
                    .map(|value| (name.clone(), value.clone()))
                    .ok_or_else(|| RuntimeError::NotReady(format!("missing secret env {name}")))
            })
            .collect::<Result<Vec<_>, _>>()
    }
}

fn legacy_host_secret_env(
    host_bridge: &HostBridge,
    agent_id: &str,
    names: &[String],
) -> Result<std::collections::HashMap<String, String>, RuntimeError> {
    let value = host_bridge.request(
        "agent/secret_env",
        Some(serde_json::json!({
            "agent_id": agent_id,
            "names": names,
        })),
    )?;
    let env = value
        .get("env")
        .and_then(|value| value.as_object())
        .ok_or_else(|| RuntimeError::InvalidParams("agent secret env".to_string()))?;
    names
        .iter()
        .map(|name| {
            env.get(name)
                .and_then(|value| value.as_str())
                .map(|value| (name.clone(), value.to_string()))
                .ok_or_else(|| RuntimeError::NotReady(format!("missing secret env {name}")))
        })
        .collect()
}

fn command_in_path(command: &str) -> bool {
    let Some(paths) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&paths).any(|path| path.join(command).is_file())
}

fn command_has_path_separator(command: &str) -> bool {
    command.contains('/') || command.contains('\\')
}

fn command_not_found_error(command: &str) -> RuntimeError {
    let executable = Path::new(command)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(command);
    RuntimeError::SetupRequired(format!(
        "Agent command not found: {executable}. Check the Agent command or install it so it is available on PATH."
    ))
}
