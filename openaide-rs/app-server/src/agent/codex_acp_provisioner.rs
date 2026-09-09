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

mod runtime_patch;
use runtime_patch::{manifest as runtime_manifest, ArtifactDigests};
const MANAGED_MARKER: &str = ".openaide-managed";
const DEFAULT_INSTALL_TIMEOUT: Duration = Duration::from_secs(120);
// The package remains pinned by npm; the embedded patch manifest separately
// identifies the immutable runtime cache so live upstream installations survive.
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
    windows: bool,
    artifacts: ArtifactDigests,
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
            windows: cfg!(windows),
            artifacts: ArtifactDigests::default(),
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
            windows: cfg!(windows),
            artifacts: ArtifactDigests::for_fixture(tests::INDEX_FIXTURE, tests::HELPER_FIXTURE),
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
            windows: cfg!(windows),
            artifacts: ArtifactDigests::for_fixture(tests::INDEX_FIXTURE, tests::HELPER_FIXTURE),
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
            windows: cfg!(windows),
            artifacts: ArtifactDigests::for_fixture(tests::INDEX_FIXTURE, tests::HELPER_FIXTURE),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_installer_for_platform(
        storage_root: PathBuf,
        installer: Arc<dyn CodexAcpInstaller>,
        windows: bool,
    ) -> Self {
        Self {
            storage_root,
            installer,
            timeout: DEFAULT_INSTALL_TIMEOUT,
            statuses: AgentStatusCache::default(),
            windows,
            artifacts: ArtifactDigests::for_fixture(tests::INDEX_FIXTURE, tests::HELPER_FIXTURE),
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
        let version_root = runtimes_root.join(&runtime_manifest().runtime_id);
        let cache_hit = valid_installation(&version_root, self.windows, &self.artifacts);
        let previous_status = (!cache_hit).then(|| self.statuses.begin_installation("codex"));
        let started_at = Instant::now();
        if !cache_hit {
            crate::logging::info(
                "codex_acp_provision_started",
                json!({
                    "agent_id": "codex",
                    "version": runtime_manifest().package_version,
                    "runtime_id": runtime_manifest().runtime_id,
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
                            "version": runtime_manifest().package_version,
                            "runtime_id": runtime_manifest().runtime_id,
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
                            "version": runtime_manifest().package_version,
                            "runtime_id": runtime_manifest().runtime_id,
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
        cleanup_stale_staging(runtimes_root);

        if !valid_installation(version_root, self.windows, &self.artifacts) {
            if version_root.exists() {
                remove_invalid_installation(version_root)?;
            }
            let staging = runtimes_root.join(format!(
                ".{}.installing-{}",
                runtime_manifest().runtime_id,
                uuid::Uuid::new_v4()
            ));
            fs::create_dir_all(&staging).map_err(provisioning_io_error)?;
            let result = self
                .installer
                .install(&staging)
                .map_err(provisioning_installer_error)
                .and_then(|_| {
                    validate_package(&staging)?;
                    self.artifacts.validate(&staging)?;
                    validate_platform_runtime(&staging, self.windows)?;
                    fs::write(staging.join(MANAGED_MARKER), managed_marker())
                        .map_err(provisioning_io_error)?;
                    fs::rename(&staging, version_root).map_err(provisioning_io_error)
                });
            if result.is_err() {
                let _ = fs::remove_dir_all(&staging);
            }
            result?;
        }
        // Acquire the running owner's lease before allowing a newer provisioner
        // to prune this directory. Publication and ownership share the install lock.
        let lease = Arc::new(open_lock_file(&version_root.join(".lease"))?);
        FileExt::lock_shared(lease.as_ref()).map_err(provisioning_io_error)?;
        prune_old_versions(runtimes_root, &runtime_manifest().runtime_id);
        FileExt::unlock(&install_lock).map_err(provisioning_io_error)?;
        let entrypoint = package_root(version_root).join("dist/index.js");
        let mut config = config;
        config.command = resolved_command_or_name("node");
        // Rust may preserve Windows' `\\?\` verbatim prefix after the managed
        // installation is published. Node interprets the slash-normalized
        // `//?/C:/...` form as a drive-relative `C:` entrypoint, so hand this
        // process boundary an ordinary Node-compatible Windows path.
        config.args = vec![process_path_argument(&entrypoint, self.windows)];
        if self.windows {
            // The @openai/codex Node launcher can terminate when nested under
            // codex-acp on Windows. Use its pinned native binary directly.
            config.env.retain(|(name, _)| name != "CODEX_PATH");
            config.env.push((
                "CODEX_PATH".to_string(),
                process_path_argument(&windows_codex_binary(version_root), true),
            ));
        }
        Ok(PreparedCodexAcpLaunch {
            config,
            lease: Some(lease),
        })
    }

    pub(crate) fn is_provisioned(&self) -> bool {
        valid_installation(
            &self
                .storage_root
                .join("agent-runtimes")
                .join("codex-acp")
                .join(&runtime_manifest().runtime_id),
            self.windows,
            &self.artifacts,
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
                Ok(Some(status)) if status.success() => break,
                Ok(Some(_)) => return Err("npm installation failed".to_string()),
                Ok(None) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(50));
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("npm installation timed out".to_string());
                }
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("npm installation status was unavailable".to_string());
                }
            }
        }
        runtime_patch::apply(destination, deadline)
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
        .map_err(|_| "installer lockfile could not be written".to_string())?;
    runtime_patch::write_assets(destination)
}

fn managed_marker() -> String {
    format!(
        "{}@{}\n",
        runtime_manifest().package_name,
        runtime_manifest().runtime_id
    )
}

fn package_root(version_root: &Path) -> PathBuf {
    version_root.join("node_modules/@openaide/codex-acp")
}

fn valid_installation(version_root: &Path, windows: bool, artifacts: &ArtifactDigests) -> bool {
    fs::read_to_string(version_root.join(MANAGED_MARKER))
        .ok()
        .as_deref()
        == Some(managed_marker().as_str())
        && validate_package(version_root).is_ok()
        && artifacts.validate(version_root).is_ok()
        && validate_platform_runtime(version_root, windows).is_ok()
}

fn windows_codex_binary(version_root: &Path) -> PathBuf {
    version_root
        .join("node_modules/@openai/codex-win32-x64")
        .join("vendor/x86_64-pc-windows-msvc/bin/codex.exe")
}

fn process_path_argument(path: &Path, windows: bool) -> String {
    let value = path.to_string_lossy();
    if windows {
        let normalized = value.replace('\\', "/");
        if let Some(path) = normalized.strip_prefix("//?/UNC/") {
            format!("//{path}")
        } else {
            normalized
                .strip_prefix("//?/")
                .unwrap_or(&normalized)
                .to_string()
        }
    } else {
        value.into_owned()
    }
}

fn validate_platform_runtime(version_root: &Path, windows: bool) -> Result<(), RuntimeError> {
    if windows && !windows_codex_binary(version_root).is_file() {
        return Err(provisioning_error(
            "installed Codex integration did not provide its native Windows runtime".to_string(),
        ));
    }
    Ok(())
}

fn validate_package(version_root: &Path) -> Result<(), RuntimeError> {
    let package_root = package_root(version_root);
    let manifest =
        fs::read_to_string(package_root.join("package.json")).map_err(provisioning_io_error)?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest)
        .map_err(|_| provisioning_error("installed package manifest is invalid".to_string()))?;
    if manifest.get("name").and_then(serde_json::Value::as_str)
        != Some(runtime_manifest().package_name.as_str())
        || manifest.get("version").and_then(serde_json::Value::as_str)
            != Some(runtime_manifest().package_version.as_str())
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
        // Lock files preserve their inode and contents are intentionally irrelevant.
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(provisioning_io_error)
}

fn remove_invalid_installation(version_root: &Path) -> Result<(), RuntimeError> {
    let lease = open_lock_file(&version_root.join(".lease"))?;
    match lease.try_lock_exclusive() {
        Ok(()) => {
            // Windows cannot delete an open lease file. The caller still holds
            // the install lock, so no new launch can acquire a shared lease here.
            drop(lease);
            fs::remove_dir_all(version_root).map_err(provisioning_io_error)
        }
        Err(error) if lock_error_is_contention(&error, cfg!(windows)) => Err(provisioning_error(
            "the managed runtime is in use and cannot be repaired until its owner stops"
                .to_string(),
        )),
        Err(error) => Err(provisioning_io_error(error)),
    }
}

fn lock_exclusive_until(file: &File, deadline: Instant) -> Result<(), RuntimeError> {
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => return Ok(()),
            Err(error) if lock_error_is_contention(&error, cfg!(windows)) => {
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

fn lock_error_is_contention(error: &std::io::Error, windows: bool) -> bool {
    // fs2 0.4 surfaces LockFileEx contention as ERROR_LOCK_VIOLATION rather
    // than WouldBlock. That is a transient owner to wait for, not a storage
    // failure users should have to retry after provisioning completes.
    error.kind() == std::io::ErrorKind::WouldBlock || (windows && error.raw_os_error() == Some(33))
}

fn cleanup_stale_staging(runtimes_root: &Path) {
    let Ok(entries) = fs::read_dir(runtimes_root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') && name.contains(".installing-") {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

fn prune_old_versions(runtimes_root: &Path, current: &str) {
    let Ok(entries) = fs::read_dir(runtimes_root) else {
        return;
    };
    let mut versions = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            (entry.path().is_dir() && name != current)
                .then(|| version_key(&name).map(|key| (key, entry.path())))
                .flatten()
        })
        .collect::<Vec<_>>();
    versions.sort_by(|left, right| right.0.cmp(&left.0));

    for (_, version_root) in versions.into_iter().skip(1) {
        prune_unleased_version(&version_root);
    }
}

fn version_key(version: &str) -> Option<(u64, u64, u64, String)> {
    let numeric = version
        .split_once('-')
        .map_or(version, |(numeric, _)| numeric);
    let mut parts = numeric.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    parts
        .next()
        .is_none()
        .then(|| (major, minor, patch, version.to_string()))
}

#[cfg(not(windows))]
fn prune_unleased_version(version_root: &Path) {
    let Ok(lease) = open_lock_file(&version_root.join(".lease")) else {
        return;
    };
    if lease.try_lock_exclusive().is_ok() {
        let _ = fs::remove_dir_all(version_root);
    }
}

#[cfg(windows)]
fn prune_unleased_version(_version_root: &Path) {
    // Windows cannot safely remove a directory containing an open lease file.
    // Retaining old versions is preferable to racing a still-running App Server.
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
