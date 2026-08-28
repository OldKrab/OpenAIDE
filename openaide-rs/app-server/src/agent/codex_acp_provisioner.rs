use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use fs2::FileExt;
use serde_json::json;

use crate::agent::acp_agent_config::{
    is_windows_batch_file, resolved_command_or_name, AcpAgentConfig,
};
use crate::agent::status_cache::AgentStatusCache;
use crate::protocol::errors::RuntimeError;

pub(crate) const CODEX_ACP_VERSION: &str = "1.1.5";
const CODEX_ACP_PACKAGE: &str = "@openaide/codex-acp";
const MANAGED_MARKER: &str = ".openaide-managed";
const DEFAULT_INSTALL_TIMEOUT: Duration = Duration::from_secs(120);
const INSTALLER_PACKAGE_JSON: &str = include_str!("../../assets/codex-acp-runtime/package.json");
const INSTALLER_PACKAGE_LOCK: &str =
    include_str!("../../assets/codex-acp-runtime/package-lock.json");

#[derive(Debug)]
pub(crate) struct PreparedCodexAcpLaunch {
    pub(crate) config: AcpAgentConfig,
    pub(crate) lease: Option<Arc<File>>,
}

pub(crate) trait CodexAcpInstaller: Send + Sync {
    fn install(&self, destination: &Path) -> Result<(), String>;
}

#[derive(Clone)]
pub(crate) struct CodexAcpProvisioner {
    storage_root: PathBuf,
    installer: Arc<dyn CodexAcpInstaller>,
    timeout: Duration,
    statuses: AgentStatusCache,
}

impl CodexAcpProvisioner {
    pub(crate) fn new_with_statuses(storage_root: PathBuf, statuses: AgentStatusCache) -> Self {
        Self {
            storage_root,
            installer: Arc::new(NpmCodexAcpInstaller {
                timeout: DEFAULT_INSTALL_TIMEOUT,
            }),
            timeout: DEFAULT_INSTALL_TIMEOUT,
            statuses,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_installer(
        storage_root: PathBuf,
        installer: Arc<dyn CodexAcpInstaller>,
    ) -> Self {
        Self {
            storage_root,
            installer,
            timeout: DEFAULT_INSTALL_TIMEOUT,
            statuses: AgentStatusCache::default(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_installer_and_timeout(
        storage_root: PathBuf,
        installer: Arc<dyn CodexAcpInstaller>,
        timeout: Duration,
    ) -> Self {
        Self {
            storage_root,
            installer,
            timeout,
            statuses: AgentStatusCache::default(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_installer_and_statuses(
        storage_root: PathBuf,
        installer: Arc<dyn CodexAcpInstaller>,
        statuses: AgentStatusCache,
    ) -> Self {
        Self {
            storage_root,
            installer,
            timeout: DEFAULT_INSTALL_TIMEOUT,
            statuses,
        }
    }

    /// Resolves the product-pinned Codex declaration to one validated managed installation.
    /// Custom Agents and test runtimes keep their configured launch unchanged.
    pub(crate) fn prepare(
        &self,
        config: AcpAgentConfig,
    ) -> Result<PreparedCodexAcpLaunch, RuntimeError> {
        if !config.uses_product_pinned_codex_package() {
            return Ok(PreparedCodexAcpLaunch {
                config,
                lease: None,
            });
        }

        let runtimes_root = self.storage_root.join("agent-runtimes").join("codex-acp");
        fs::create_dir_all(&runtimes_root).map_err(provisioning_io_error)?;
        let version_root = runtimes_root.join(CODEX_ACP_VERSION);
        let cache_hit = valid_installation(&version_root);
        let previous_status = (!cache_hit).then(|| self.statuses.begin_installation("codex"));
        let started_at = Instant::now();
        if !cache_hit {
            crate::logging::info(
                "codex_acp_provision_started",
                json!({
                    "agent_id": "codex",
                    "version": CODEX_ACP_VERSION,
                    "attempt": 1,
                    "cache_hit": false,
                }),
            );
        }

        let result = self.prepare_managed_launch(config, &runtimes_root, &version_root);
        match result {
            Ok(launch) => {
                if let Some(previous) = previous_status {
                    self.statuses.complete_installation("codex", previous);
                    crate::logging::info(
                        "codex_acp_provision_completed",
                        json!({
                            "agent_id": "codex",
                            "version": CODEX_ACP_VERSION,
                            "attempt": 1,
                            "cache_hit": false,
                            "outcome_kind": "installed",
                            "duration_ms": started_at.elapsed().as_millis(),
                        }),
                    );
                } else {
                    self.statuses.record_launching("codex");
                }
                Ok(launch)
            }
            Err(error) => {
                if previous_status.is_some() {
                    crate::logging::warn(
                        "codex_acp_provision_failed",
                        json!({
                            "agent_id": "codex",
                            "version": CODEX_ACP_VERSION,
                            "attempt": 1,
                            "cache_hit": false,
                            "outcome_kind": error.reason(),
                            "duration_ms": started_at.elapsed().as_millis(),
                        }),
                    );
                }
                self.statuses.record_probe_error("codex", &error);
                Err(error)
            }
        }
    }

    fn prepare_managed_launch(
        &self,
        config: AcpAgentConfig,
        runtimes_root: &Path,
        version_root: &Path,
    ) -> Result<PreparedCodexAcpLaunch, RuntimeError> {
        let install_lock = open_lock_file(&runtimes_root.join(".install.lock"))?;
        lock_exclusive_until(&install_lock, Instant::now() + self.timeout)?;

        if !valid_installation(version_root) {
            if version_root.exists() {
                fs::remove_dir_all(version_root).map_err(provisioning_io_error)?;
            }
            let staging = runtimes_root.join(format!(
                ".{CODEX_ACP_VERSION}.installing-{}",
                uuid::Uuid::new_v4()
            ));
            fs::create_dir_all(&staging).map_err(provisioning_io_error)?;
            let result = self
                .installer
                .install(&staging)
                .map_err(provisioning_installer_error)
                .and_then(|_| {
                    validate_package(&staging)?;
                    fs::write(staging.join(MANAGED_MARKER), managed_marker())
                        .map_err(provisioning_io_error)?;
                    fs::rename(&staging, version_root).map_err(provisioning_io_error)
                });
            if result.is_err() {
                let _ = fs::remove_dir_all(&staging);
            }
            result?;
        }
        FileExt::unlock(&install_lock).map_err(provisioning_io_error)?;

        let lease = Arc::new(open_lock_file(&version_root.join(".lease"))?);
        FileExt::lock_shared(lease.as_ref()).map_err(provisioning_io_error)?;
        let entrypoint = package_root(version_root).join("dist/index.js");
        Ok(PreparedCodexAcpLaunch {
            config: AcpAgentConfig {
                command: resolved_command_or_name("node"),
                args: vec![entrypoint.to_string_lossy().into_owned()],
                ..config
            },
            lease: Some(lease),
        })
    }

    pub(crate) fn is_provisioned(&self) -> bool {
        valid_installation(
            &self
                .storage_root
                .join("agent-runtimes")
                .join("codex-acp")
                .join(CODEX_ACP_VERSION),
        )
    }
}

struct NpmCodexAcpInstaller {
    timeout: Duration,
}

impl CodexAcpInstaller for NpmCodexAcpInstaller {
    fn install(&self, destination: &Path) -> Result<(), String> {
        write_installer_manifest(destination)?;
        let npm = resolved_command_or_name("npm");
        let mut command = if cfg!(windows) && is_windows_batch_file(&npm) {
            let mut command = Command::new("cmd.exe");
            command.args(["/D", "/C"]).arg(npm);
            command
        } else {
            Command::new(npm)
        };
        let mut child = command
            .args([
                "ci",
                "--ignore-scripts",
                "--omit=dev",
                "--no-audit",
                "--no-fund",
            ])
            .current_dir(destination)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "npm could not be started".to_string())?;
        let deadline = Instant::now() + self.timeout;
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => return Ok(()),
                Ok(Some(_)) => return Err("npm installation failed".to_string()),
                Ok(None) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(50));
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("npm installation timed out".to_string());
                }
                Err(_) => return Err("npm installation status was unavailable".to_string()),
            }
        }
    }
}

fn write_installer_manifest(destination: &Path) -> Result<(), String> {
    let mut package = File::create(destination.join("package.json"))
        .map_err(|_| "installer manifest could not be created".to_string())?;
    package
        .write_all(INSTALLER_PACKAGE_JSON.as_bytes())
        .map_err(|_| "installer manifest could not be written".to_string())?;
    let mut lock = File::create(destination.join("package-lock.json"))
        .map_err(|_| "installer lockfile could not be created".to_string())?;
    lock.write_all(INSTALLER_PACKAGE_LOCK.as_bytes())
        .map_err(|_| "installer lockfile could not be written".to_string())
}

fn managed_marker() -> String {
    format!("{CODEX_ACP_PACKAGE}@{CODEX_ACP_VERSION}\n")
}

fn package_root(version_root: &Path) -> PathBuf {
    version_root.join("node_modules/@openaide/codex-acp")
}

fn valid_installation(version_root: &Path) -> bool {
    fs::read_to_string(version_root.join(MANAGED_MARKER))
        .ok()
        .as_deref()
        == Some(managed_marker().as_str())
        && validate_package(version_root).is_ok()
}

fn validate_package(version_root: &Path) -> Result<(), RuntimeError> {
    let package_root = package_root(version_root);
    let manifest =
        fs::read_to_string(package_root.join("package.json")).map_err(provisioning_io_error)?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest)
        .map_err(|_| provisioning_error("installed package manifest is invalid".to_string()))?;
    if manifest.get("name").and_then(serde_json::Value::as_str) != Some(CODEX_ACP_PACKAGE)
        || manifest.get("version").and_then(serde_json::Value::as_str) != Some(CODEX_ACP_VERSION)
        || !package_root.join("dist/index.js").is_file()
    {
        return Err(provisioning_error(
            "installed Codex integration did not match the pinned package".to_string(),
        ));
    }
    Ok(())
}

fn open_lock_file(path: &Path) -> Result<File, RuntimeError> {
    OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(path)
        .map_err(provisioning_io_error)
}

fn lock_exclusive_until(file: &File, deadline: Instant) -> Result<(), RuntimeError> {
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(provisioning_error(
                        "another installation did not finish in time".to_string(),
                    ));
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(provisioning_io_error(error)),
        }
    }
}

fn provisioning_io_error(_: std::io::Error) -> RuntimeError {
    provisioning_error("managed Codex integration storage is unavailable".to_string())
}

fn provisioning_error(message: String) -> RuntimeError {
    RuntimeError::NotReady(format!("Codex integration setup failed: {message}"))
}

fn provisioning_installer_error(message: String) -> RuntimeError {
    if message == "npm could not be started" {
        RuntimeError::NodeJsRequired(
            "Codex needs Node.js before its integration can be installed.".to_string(),
        )
    } else {
        provisioning_error(message)
    }
}

#[cfg(test)]
#[path = "codex_acp_provisioner_tests.rs"]
mod tests;
