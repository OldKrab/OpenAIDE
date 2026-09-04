use std::env;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use agent_client_protocol::AcpAgent;

use crate::agent::acp_trace::AcpTraceSession;
use crate::agent::AgentSecretResolver;
use crate::protocol::errors::RuntimeError;
use crate::protocol::host::HostBridge;

#[cfg(test)]
#[path = "acp_agent_config_tests.rs"]
mod tests;

const PRODUCT_CODEX_ACP_SPEC: &str = "@openaide/codex-acp@1.2.0";

#[derive(Debug, Clone)]
pub struct AcpAgentConfig {
    pub agent_id: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub secret_env: Vec<String>,
}

impl AcpAgentConfig {
    /// The built-in Codex adapter is product-controlled and version-pinned.
    ///
    /// A `codex-acp` executable found on PATH is intentionally not used here:
    /// users who need a different adapter build can register it as a Custom
    /// Agent, while the built-in entry stays reproducible across releases.
    pub fn codex() -> Self {
        Self::codex_managed_package()
    }

    /// This declaration identifies the locked package policy. Production resolves it
    /// to the per-user managed installation before any process command is evaluated.
    pub(crate) fn codex_managed_package() -> Self {
        Self {
            agent_id: "codex".to_string(),
            command: resolved_command_or_name("npx"),
            args: vec!["-y".to_string(), PRODUCT_CODEX_ACP_SPEC.to_string()],
            env: Vec::new(),
            secret_env: Vec::new(),
        }
    }

    pub fn opencode() -> Self {
        if let Some(command) = resolve_command_in_path("opencode") {
            Self {
                agent_id: "opencode".to_string(),
                command: command.to_string_lossy().into_owned(),
                args: vec!["acp".to_string()],
                env: Vec::new(),
                secret_env: Vec::new(),
            }
        } else {
            Self {
                agent_id: "opencode".to_string(),
                command: resolved_command_or_name("npx"),
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
        let args = process_args(&self.command, &self.args, &env, cfg!(windows));
        let agent = AcpAgent::from_args(args).map_err(super::acp_errors::acp_error)?;
        Ok(match trace {
            Some(trace) => agent.with_debug(move |line, direction| {
                trace.record_line(line, direction);
            }),
            None => agent,
        })
    }

    pub(crate) fn ensure_command_available(&self) -> Result<(), RuntimeError> {
        if command_has_path_separator(&self.command) {
            let path = Path::new(&self.command);
            if !path.is_file() {
                return Err(command_not_found_error(&self.agent_id, &self.command));
            }
            return Ok(());
        }
        if command_in_path(&self.command) {
            Ok(())
        } else {
            Err(command_not_found_error(&self.agent_id, &self.command))
        }
    }

    pub(crate) fn diagnostic_launcher_kind(&self) -> &'static str {
        let command_name = Path::new(&self.command).file_stem().and_then(OsStr::to_str);
        if command_name.is_some_and(|name| name.eq_ignore_ascii_case("npx"))
            && self.args == ["-y", PRODUCT_CODEX_ACP_SPEC]
        {
            "managed_package"
        } else {
            "configured_command"
        }
    }

    pub(crate) fn uses_product_pinned_codex_package(&self) -> bool {
        self.agent_id == "codex"
            && Path::new(&self.command)
                .file_stem()
                .and_then(OsStr::to_str)
                .is_some_and(|name| name.eq_ignore_ascii_case("npx"))
            && self.args == ["-y", PRODUCT_CODEX_ACP_SPEC]
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
    resolve_command_in_path(command).is_some()
}

pub(super) fn resolved_command_or_name(command: &str) -> String {
    // Unix can execute the command name through PATH. Windows needs the
    // resolved launcher suffix (usually `.cmd`) so CreateProcess can delegate
    // it through `cmd.exe` reliably.
    if !cfg!(windows) {
        return command.to_string();
    }
    resolve_command_in_path(command)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| command.to_string())
}

fn process_args(
    command: &str,
    args: &[String],
    env: &[(String, String)],
    windows: bool,
) -> Vec<String> {
    let mut process_args = env
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>();
    if windows && is_windows_batch_file(command) {
        // CreateProcess cannot execute batch files directly. Disable AutoRun
        // hooks and delegate only the npm launcher invocation to cmd.exe.
        process_args.extend(["cmd.exe".to_string(), "/D".to_string(), "/C".to_string()]);
    }
    process_args.push(command.to_string());
    process_args.extend(args.iter().cloned());
    process_args
}

pub(super) fn is_windows_batch_file(command: &str) -> bool {
    Path::new(command)
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat")
        })
}

fn resolve_command_in_path(command: &str) -> Option<PathBuf> {
    let paths = env::var_os("PATH")?;
    resolve_command_in_paths(command, env::split_paths(&paths), command_extensions())
}

fn resolve_command_in_paths<P, E>(
    command: &str,
    paths: impl IntoIterator<Item = P>,
    extensions: impl IntoIterator<Item = E>,
) -> Option<PathBuf>
where
    P: AsRef<Path>,
    E: AsRef<OsStr>,
{
    let extensions = if Path::new(command).extension().is_some() {
        vec![OsString::new()]
    } else {
        extensions
            .into_iter()
            .map(|extension| extension.as_ref().to_os_string())
            .collect::<Vec<_>>()
    };
    paths.into_iter().find_map(|path| {
        extensions.iter().find_map(|extension| {
            let mut executable = OsString::from(command);
            executable.push(extension);
            let candidate = path.as_ref().join(executable);
            candidate.is_file().then_some(candidate)
        })
    })
}

#[cfg(windows)]
fn command_extensions() -> Vec<OsString> {
    // Windows cannot execute the extensionless POSIX shims installed alongside
    // npm's native launchers. Prefer formats that CreateProcess or cmd.exe can run.
    windows_command_extensions(env::var_os("PATHEXT").as_deref())
}

#[cfg(not(windows))]
fn command_extensions() -> Vec<OsString> {
    vec![OsString::new()]
}

#[cfg(any(windows, test))]
fn windows_command_extensions(pathext: Option<&OsStr>) -> Vec<OsString> {
    const DEFAULT: [&str; 4] = [".COM", ".EXE", ".BAT", ".CMD"];
    let supported = |extension: &str| {
        DEFAULT
            .iter()
            .any(|candidate| extension.eq_ignore_ascii_case(candidate))
    };
    let mut extensions = pathext
        .and_then(OsStr::to_str)
        .into_iter()
        .flat_map(|value| value.split(';'))
        .map(str::trim)
        .filter(|extension| supported(extension))
        .map(OsString::from)
        .collect::<Vec<_>>();
    if extensions.is_empty() {
        extensions.extend(DEFAULT.into_iter().map(OsString::from));
    }
    extensions
}

fn command_has_path_separator(command: &str) -> bool {
    command.contains('/') || command.contains('\\')
}

fn command_not_found_error(agent_id: &str, command: &str) -> RuntimeError {
    let executable = Path::new(command)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(command);
    if agent_id == "codex"
        && ["node", "npm", "npx"]
            .iter()
            .any(|candidate| executable.eq_ignore_ascii_case(candidate))
    {
        return RuntimeError::NodeJsRequired(
            "Codex needs Node.js before it can start.".to_string(),
        );
    }
    RuntimeError::SetupRequired(format!(
        "Agent command not found: {executable}. Check the Agent command or install it so it is available on PATH."
    ))
}
