//! Installs the product-pinned ACP adapters into per-user storage (ADR 0007).
//!
//! One provisioner serves every [`ManagedAcpRuntime`]. Each runtime owns its own
//! directory under `agent-runtimes/`, so their install locks, leases, and pruning are
//! independent. An installation is staged with `npm ci` from the embedded lockfile,
//! receives the embedded adapter package when the runtime has one (ADR-0079), is
//! validated, marked, and published by rename; a launch then holds a shared lease on
//! it so a newer App Server cannot prune a version still running.

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

mod embedded;
mod runtime;
mod runtime_patch;
use embedded::AdapterFault;
pub(crate) use runtime::ManagedAcpRuntime;
use runtime_patch::ArtifactDigests;
const MANAGED_MARKER: &str = ".openaide-managed";

#[derive(Debug)]
pub(crate) struct PreparedManagedAcpLaunch {
    pub(crate) config: AcpAgentConfig,
    pub(crate) lease: Option<Arc<File>>,
}

pub(crate) trait ManagedAcpInstaller: Send + Sync {
    fn install(&self, runtime: ManagedAcpRuntime, destination: &Path) -> Result<(), String>;
}

#[derive(Clone)]
pub(crate) struct ManagedAcpProvisioner {
    storage_root: PathBuf,
    installer: Arc<dyn ManagedAcpInstaller>,
    /// Replaces each runtime's own install bound; tests shorten lock waits with it.
    timeout: Option<Duration>,
    statuses: AgentStatusCache,
    windows: bool,
    /// Expected digests of the patched Codex files.
    artifacts: ArtifactDigests,
}

impl ManagedAcpProvisioner {
    pub(crate) fn new_with_statuses(storage_root: PathBuf, statuses: AgentStatusCache) -> Self {
        Self {
            storage_root,
            installer: Arc::new(NpmManagedAcpInstaller),
            timeout: None,
            statuses,
            windows: cfg!(windows),
            artifacts: ArtifactDigests::default(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_installer(
        storage_root: PathBuf,
        installer: Arc<dyn ManagedAcpInstaller>,
    ) -> Self {
        Self {
            storage_root,
            installer,
            timeout: None,
            statuses: AgentStatusCache::default(),
            windows: cfg!(windows),
            artifacts: ArtifactDigests::for_fixture(tests::INDEX_FIXTURE, tests::HELPER_FIXTURE),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_installer_and_timeout(
        storage_root: PathBuf,
        installer: Arc<dyn ManagedAcpInstaller>,
        timeout: Duration,
    ) -> Self {
        Self {
            storage_root,
            installer,
            timeout: Some(timeout),
            statuses: AgentStatusCache::default(),
            windows: cfg!(windows),
            artifacts: ArtifactDigests::for_fixture(tests::INDEX_FIXTURE, tests::HELPER_FIXTURE),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_installer_and_statuses(
        storage_root: PathBuf,
        installer: Arc<dyn ManagedAcpInstaller>,
        statuses: AgentStatusCache,
    ) -> Self {
        Self {
            storage_root,
            installer,
            timeout: None,
            statuses,
            windows: cfg!(windows),
            artifacts: ArtifactDigests::for_fixture(tests::INDEX_FIXTURE, tests::HELPER_FIXTURE),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_installer_for_platform(
        storage_root: PathBuf,
        installer: Arc<dyn ManagedAcpInstaller>,
        windows: bool,
    ) -> Self {
        Self {
            storage_root,
            installer,
            timeout: None,
            statuses: AgentStatusCache::default(),
            windows,
            artifacts: ArtifactDigests::for_fixture(tests::INDEX_FIXTURE, tests::HELPER_FIXTURE),
        }
    }

    /// Resolves a product-pinned declaration to one validated managed installation.
    /// Custom Agents and test runtimes keep their configured launch unchanged.
    pub(crate) fn prepare(
        &self,
        config: AcpAgentConfig,
    ) -> Result<PreparedManagedAcpLaunch, RuntimeError> {
        let Some(runtime) = config.managed_runtime() else {
            return Ok(PreparedManagedAcpLaunch {
                config,
                lease: None,
            });
        };

        let agent_id = runtime.agent_id();
        let package = runtime.package();
        let runtimes_root = self.runtimes_root(runtime);
        fs::create_dir_all(&runtimes_root)
            .map_err(|error| provisioning_io_error(runtime, error))?;
        let version_root = runtimes_root.join(&package.runtime_id);
        let cache_hit = self.valid_installation(runtime, &version_root, Check::Launch);
        let previous_status = (!cache_hit).then(|| self.statuses.begin_installation(agent_id));
        let started_at = Instant::now();
        let fields = |extra: serde_json::Value| {
            let mut fields = json!({
                "agent_id": agent_id,
                "version": package.version,
                "runtime_id": package.runtime_id,
                "attempt": 1,
                "cache_hit": false,
            });
            if let (Some(fields), serde_json::Value::Object(extra)) =
                (fields.as_object_mut(), extra)
            {
                fields.extend(extra);
            }
            fields
        };
        if !cache_hit {
            crate::logging::info(&runtime.event("started"), fields(json!({})));
        }

        let result = self.prepare_managed_launch(runtime, config, &runtimes_root, &version_root);
        match result {
            Ok(launch) => {
                if let Some(previous) = previous_status {
                    self.statuses.complete_installation(agent_id, previous);
                    crate::logging::info(
                        &runtime.event("completed"),
                        fields(json!({
                            "outcome_kind": "installed",
                            "duration_ms": started_at.elapsed().as_millis(),
                        })),
                    );
                } else {
                    self.statuses.record_launching(agent_id);
                }
                Ok(launch)
            }
            Err(error) => {
                if previous_status.is_some() {
                    crate::logging::warn(
                        &runtime.event("failed"),
                        fields(json!({
                            "outcome_kind": error.reason(),
                            "duration_ms": started_at.elapsed().as_millis(),
                        })),
                    );
                }
                self.statuses.record_probe_error(agent_id, &error);
                Err(error)
            }
        }
    }

    fn prepare_managed_launch(
        &self,
        runtime: ManagedAcpRuntime,
        config: AcpAgentConfig,
        runtimes_root: &Path,
        version_root: &Path,
    ) -> Result<PreparedManagedAcpLaunch, RuntimeError> {
        let io_error = |error| provisioning_io_error(runtime, error);
        let runtime_id = &runtime.package().runtime_id;
        let install_lock = open_lock_file(runtime, &runtimes_root.join(".install.lock"))?;
        let timeout = self.timeout.unwrap_or_else(|| runtime.install_timeout());
        lock_exclusive_until(runtime, &install_lock, Instant::now() + timeout)?;
        cleanup_stale_staging(runtimes_root);

        if !self.valid_installation(runtime, version_root, Check::Passive) {
            if version_root.exists() {
                remove_invalid_installation(runtime, version_root)?;
            }
            let staging =
                runtimes_root.join(format!(".{runtime_id}.installing-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&staging).map_err(io_error)?;
            let result = self
                .installer
                .install(runtime, &staging)
                .map_err(|message| provisioning_installer_error(runtime, message))
                .and_then(|_| unpack_embedded_adapter(runtime, &staging))
                .and_then(|_| {
                    self.validate_installation(runtime, &staging, Check::Staged)?;
                    fs::write(staging.join(MANAGED_MARKER), managed_marker(runtime))
                        .map_err(io_error)?;
                    fs::rename(&staging, version_root).map_err(io_error)
                });
            if result.is_err() {
                let _ = fs::remove_dir_all(&staging);
            }
            result?;
        }
        // Acquire the running owner's lease before allowing a newer provisioner
        // to prune this directory. Publication and ownership share the install lock.
        let lease = Arc::new(open_lock_file(runtime, &version_root.join(".lease"))?);
        FileExt::lock_shared(lease.as_ref()).map_err(io_error)?;
        prune_old_versions(runtimes_root, runtime_id);
        FileExt::unlock(&install_lock).map_err(io_error)?;
        let entrypoint = package_root(runtime, version_root).join("dist/index.js");
        let mut config = config;
        config.command = resolved_command_or_name("node");
        // Rust may preserve Windows' `\\?\` verbatim prefix after the managed
        // installation is published. Node interprets the slash-normalized
        // `//?/C:/...` form as a drive-relative `C:` entrypoint, so hand this
        // process boundary an ordinary Node-compatible Windows path.
        config.args = vec![process_path_argument(&entrypoint, self.windows)];
        if runtime == ManagedAcpRuntime::Codex && self.windows {
            // The @openai/codex Node launcher can terminate when nested under
            // codex-acp on Windows. Use its pinned native binary directly.
            config.env.retain(|(name, _)| name != "CODEX_PATH");
            config.env.push((
                "CODEX_PATH".to_string(),
                process_path_argument(&windows_codex_binary(version_root), true),
            ));
        }
        Ok(PreparedManagedAcpLaunch {
            config,
            lease: Some(lease),
        })
    }

    /// Whether the pinned version is already installed, without installing it.
    pub(crate) fn is_provisioned(&self, runtime: ManagedAcpRuntime) -> bool {
        self.valid_installation(
            runtime,
            &self
                .runtimes_root(runtime)
                .join(&runtime.package().runtime_id),
            Check::Passive,
        )
    }

    fn runtimes_root(&self, runtime: ManagedAcpRuntime) -> PathBuf {
        self.storage_root
            .join("agent-runtimes")
            .join(runtime.runtimes_dir())
    }

    fn valid_installation(
        &self,
        runtime: ManagedAcpRuntime,
        version_root: &Path,
        check: Check,
    ) -> bool {
        fs::read_to_string(version_root.join(MANAGED_MARKER))
            .ok()
            .as_deref()
            == Some(managed_marker(runtime).as_str())
            && self
                .validate_installation(runtime, version_root, check)
                .is_ok()
    }

    fn validate_installation(
        &self,
        runtime: ManagedAcpRuntime,
        version_root: &Path,
        check: Check,
    ) -> Result<(), RuntimeError> {
        validate_embedded_adapter(runtime, version_root, check)?;
        validate_package(runtime, version_root)?;
        if runtime.has_runtime_patch() {
            self.artifacts.validate(version_root)?;
        }
        validate_platform_runtime(runtime, version_root, self.windows)
    }
}

/// Why an installation is being validated; it decides what is logged. Passive
/// checks (Native Session Discovery) stay quiet, a launch records the cached
/// installation's check, and a staged installation records its install check.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Check {
    Passive,
    Launch,
    Staged,
}

impl Check {
    fn stage(self) -> Option<&'static str> {
        match self {
            Self::Passive => None,
            Self::Launch => Some("cached"),
            Self::Staged => Some("staged"),
        }
    }
}

/// Safe metadata identifying one embedded adapter operation.
fn adapter_fields(runtime: ManagedAcpRuntime, extra: serde_json::Value) -> serde_json::Value {
    let package = runtime.package();
    let mut fields = json!({
        "agent_id": runtime.agent_id(),
        "version": package.version,
        "runtime_id": package.runtime_id,
        "commit": runtime.embedded().map(|adapter| adapter.short_commit()),
        "file_count": runtime.embedded().map(|adapter| adapter.files.len()),
    });
    if let (Some(fields), serde_json::Value::Object(extra)) = (fields.as_object_mut(), extra) {
        fields.extend(extra);
    }
    fields
}

fn adapter_fault_error(runtime: ManagedAcpRuntime, fault: AdapterFault) -> RuntimeError {
    match fault {
        AdapterFault::Io => provisioning_error(
            runtime,
            format!(
                "managed {} integration storage is unavailable",
                runtime.label()
            ),
        ),
        AdapterFault::MissingFile | AdapterFault::DigestMismatch => provisioning_error(
            runtime,
            format!(
                "installed {} integration did not match the embedded adapter",
                runtime.label()
            ),
        ),
    }
}

/// Writes the embedded adapter into a staging installation, after npm installed
/// its dependencies. The adapter never comes from npm (ADR-0079).
fn unpack_embedded_adapter(runtime: ManagedAcpRuntime, staging: &Path) -> Result<(), RuntimeError> {
    let Some(adapter) = runtime.embedded() else {
        return Ok(());
    };
    let started_at = Instant::now();
    crate::logging::info(
        &runtime.event("adapter_unpack_started"),
        adapter_fields(runtime, json!({})),
    );
    let result = adapter.unpack(&package_root(runtime, staging));
    let duration_ms = started_at.elapsed().as_millis();
    match result {
        Ok(()) => crate::logging::info(
            &runtime.event("adapter_unpack_completed"),
            adapter_fields(
                runtime,
                json!({ "outcome_kind": "unpacked", "duration_ms": duration_ms }),
            ),
        ),
        Err(fault) => crate::logging::warn(
            &runtime.event("adapter_unpack_completed"),
            adapter_fields(
                runtime,
                json!({
                    "outcome_kind": "failed",
                    "error_kind": fault.kind(),
                    "duration_ms": duration_ms,
                }),
            ),
        ),
    }
    result.map_err(|fault| adapter_fault_error(runtime, fault))
}

/// Checks an installed adapter's bytes against the digests recorded when it was
/// built. Runs for every launch, so a modified cached installation is reinstalled.
fn validate_embedded_adapter(
    runtime: ManagedAcpRuntime,
    version_root: &Path,
    check: Check,
) -> Result<(), RuntimeError> {
    let Some(adapter) = runtime.embedded() else {
        return Ok(());
    };
    let started_at = Instant::now();
    if let Some(stage) = check.stage() {
        crate::logging::info(
            &runtime.event("adapter_validate_started"),
            adapter_fields(runtime, json!({ "stage": stage })),
        );
    }
    let result = adapter.validate(&package_root(runtime, version_root));
    if let Some(stage) = check.stage() {
        let duration_ms = started_at.elapsed().as_millis();
        match result {
            Ok(()) => crate::logging::info(
                &runtime.event("adapter_validate_completed"),
                adapter_fields(
                    runtime,
                    json!({ "stage": stage, "outcome_kind": "valid", "duration_ms": duration_ms }),
                ),
            ),
            Err(fault) => crate::logging::warn(
                &runtime.event("adapter_validate_completed"),
                adapter_fields(
                    runtime,
                    json!({
                        "stage": stage,
                        "outcome_kind": "invalid",
                        "error_kind": fault.kind(),
                        "duration_ms": duration_ms,
                    }),
                ),
            ),
        }
    }
    result.map_err(|fault| adapter_fault_error(runtime, fault))
}

struct NpmManagedAcpInstaller;

impl ManagedAcpInstaller for NpmManagedAcpInstaller {
    fn install(&self, runtime: ManagedAcpRuntime, destination: &Path) -> Result<(), String> {
        write_installer_manifest(runtime, destination)?;
        let npm = resolved_command_or_name("npm");
        let mut command = if cfg!(windows) && is_windows_batch_file(&npm) {
            let mut command = Command::new("cmd.exe");
            command.args(["/D", "/C"]).arg(npm);
            command
        } else {
            Command::new(npm)
        };
        // Lifecycle scripts stay disabled: every locked package must work as extracted.
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
            .map_err(installer_spawn_error)?;
        let deadline = Instant::now() + runtime.install_timeout();
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
        if runtime.has_runtime_patch() {
            runtime_patch::apply(destination, deadline)?;
        }
        Ok(())
    }
}

fn write_installer_manifest(runtime: ManagedAcpRuntime, destination: &Path) -> Result<(), String> {
    let (package_json, package_lock) = runtime.installer_files();
    let mut package = File::create(destination.join("package.json"))
        .map_err(|_| "installer manifest could not be created".to_string())?;
    package
        .write_all(package_json.as_bytes())
        .map_err(|_| "installer manifest could not be written".to_string())?;
    let mut lock = File::create(destination.join("package-lock.json"))
        .map_err(|_| "installer lockfile could not be created".to_string())?;
    lock.write_all(package_lock.as_bytes())
        .map_err(|_| "installer lockfile could not be written".to_string())?;
    if runtime.has_runtime_patch() {
        runtime_patch::write_assets(destination)?;
    }
    Ok(())
}

fn managed_marker(runtime: ManagedAcpRuntime) -> String {
    let package = runtime.package();
    format!("{}@{}\n", package.name, package.runtime_id)
}

fn package_root(runtime: ManagedAcpRuntime, version_root: &Path) -> PathBuf {
    version_root
        .join("node_modules")
        .join(&runtime.package().name)
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

fn validate_platform_runtime(
    runtime: ManagedAcpRuntime,
    version_root: &Path,
    windows: bool,
) -> Result<(), RuntimeError> {
    let present = match runtime {
        ManagedAcpRuntime::Codex if windows => windows_codex_binary(version_root).is_file(),
        ManagedAcpRuntime::Codex => {
            runtime::unix_codex_binary(version_root).is_none_or(|binary| binary.is_file())
        }
        ManagedAcpRuntime::Claude => {
            let candidates = runtime::claude_native_binaries(version_root, windows);
            candidates.is_empty() || candidates.iter().any(|binary| binary.is_file())
        }
    };
    if present {
        Ok(())
    } else {
        Err(provisioning_error(
            runtime,
            format!(
                "installed {} integration did not provide its native runtime for this platform",
                runtime.label()
            ),
        ))
    }
}

fn validate_package(runtime: ManagedAcpRuntime, version_root: &Path) -> Result<(), RuntimeError> {
    let package = runtime.package();
    let package_root = package_root(runtime, version_root);
    let manifest = fs::read_to_string(package_root.join("package.json"))
        .map_err(|error| provisioning_io_error(runtime, error))?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest).map_err(|_| {
        provisioning_error(runtime, "installed package manifest is invalid".to_string())
    })?;
    if manifest.get("name").and_then(serde_json::Value::as_str) != Some(package.name.as_str())
        || manifest.get("version").and_then(serde_json::Value::as_str)
            != Some(package.version.as_str())
        || !package_root.join("dist/index.js").is_file()
    {
        return Err(provisioning_error(
            runtime,
            format!(
                "installed {} integration did not match the pinned package",
                runtime.label()
            ),
        ));
    }
    Ok(())
}

fn open_lock_file(runtime: ManagedAcpRuntime, path: &Path) -> Result<File, RuntimeError> {
    OpenOptions::new()
        .create(true)
        // Lock files preserve their inode and contents are intentionally irrelevant.
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| provisioning_io_error(runtime, error))
}

fn remove_invalid_installation(
    runtime: ManagedAcpRuntime,
    version_root: &Path,
) -> Result<(), RuntimeError> {
    let lease = open_lock_file(runtime, &version_root.join(".lease"))?;
    match lease.try_lock_exclusive() {
        Ok(()) => {
            // Windows cannot delete an open lease file. The caller still holds
            // the install lock, so no new launch can acquire a shared lease here.
            drop(lease);
            fs::remove_dir_all(version_root).map_err(|error| provisioning_io_error(runtime, error))
        }
        Err(error) if lock_error_is_contention(&error, cfg!(windows)) => Err(provisioning_error(
            runtime,
            "the managed runtime is in use and cannot be repaired until its owner stops"
                .to_string(),
        )),
        Err(error) => Err(provisioning_io_error(runtime, error)),
    }
}

fn lock_exclusive_until(
    runtime: ManagedAcpRuntime,
    file: &File,
    deadline: Instant,
) -> Result<(), RuntimeError> {
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => return Ok(()),
            Err(error) if lock_error_is_contention(&error, cfg!(windows)) => {
                if Instant::now() >= deadline {
                    return Err(provisioning_error(
                        runtime,
                        "another installation did not finish in time".to_string(),
                    ));
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(provisioning_io_error(runtime, error)),
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
                .then(|| {
                    version_key(&name, installed_at(&entry.path())).map(|key| (key, entry.path()))
                })
                .flatten()
        })
        .collect::<Vec<_>>();
    versions.sort_by(|left, right| right.0.cmp(&left.0));

    for (_, version_root) in versions.into_iter().skip(1) {
        prune_unleased_version(&version_root);
    }
}

/// When an installation was published; orders installations of one adapter
/// version built from different fork commits (`<version>-<commit>`), whose names
/// carry no order. Unmarked directories sort first, so they are pruned first.
fn installed_at(version_root: &Path) -> std::time::SystemTime {
    fs::metadata(version_root.join(MANAGED_MARKER))
        .and_then(|metadata| metadata.modified())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
}

type VersionKey = (u64, u64, u64, std::time::SystemTime, String);

fn version_key(version: &str, installed_at: std::time::SystemTime) -> Option<VersionKey> {
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
        .then(|| (major, minor, patch, installed_at, version.to_string()))
}

#[cfg(not(windows))]
fn prune_unleased_version(version_root: &Path) {
    let Ok(lease) = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(version_root.join(".lease"))
    else {
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

fn provisioning_io_error(runtime: ManagedAcpRuntime, _: std::io::Error) -> RuntimeError {
    provisioning_error(
        runtime,
        format!(
            "managed {} integration storage is unavailable",
            runtime.label()
        ),
    )
}

fn provisioning_error(runtime: ManagedAcpRuntime, message: String) -> RuntimeError {
    RuntimeError::NotReady(format!(
        "{} integration setup failed: {message}",
        runtime.label()
    ))
}

fn installer_spawn_error(error: std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::NotFound => "npm could not be started".to_string(),
        std::io::ErrorKind::PermissionDenied => "permission denied when starting npm".to_string(),
        _ => "npm process launch failed".to_string(),
    }
}

fn provisioning_installer_error(runtime: ManagedAcpRuntime, message: String) -> RuntimeError {
    if message == "npm could not be started" {
        RuntimeError::NodeJsRequired(
            "OpenAIDE could not find npm in its launch environment. Install Node.js with npm, or restart OpenAIDE after updating your terminal PATH.".to_string(),
        )
    } else {
        provisioning_error(runtime, message)
    }
}

#[cfg(test)]
#[path = "managed_acp_provisioner_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "managed_acp_provisioner_embedded_tests.rs"]
mod embedded_tests;
