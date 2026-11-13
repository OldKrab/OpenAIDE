//! The Claude runtime: npm installs the dependencies, and the provisioner unpacks
//! and validates the adapter embedded from its fork submodule (ADR-0079).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use fs2::FileExt;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

use super::tests::{BlockingInstaller, RecordingInstaller};
use super::{ManagedAcpInstaller, ManagedAcpProvisioner, ManagedAcpRuntime};
use crate::agent::acp_agent_config::AcpAgentConfig;
use crate::agent::status_cache::AgentStatusCache;
use crate::logging::capture_test_logs;
use openaide_app_server_protocol::snapshot::AgentStatus;

const CLAUDE_RUNTIME_PACKAGE: &str = include_str!(
    "../../../../packages/app-shell-contracts/src/agentCatalog/claude-acp/package.json"
);

/// Writes what `npm ci` leaves for the Claude runtime lockfile on this host: its
/// dependencies, including the native Claude binary, but never the adapter itself.
pub(super) fn write_claude_dependencies(destination: &Path) {
    let sdk = destination.join("node_modules/@anthropic-ai/claude-agent-sdk");
    fs::create_dir_all(&sdk).expect("create Claude SDK fixture");
    fs::write(sdk.join("package.json"), "{}").expect("write Claude SDK fixture");
    if let Some(binary) = super::runtime::claude_native_binaries(destination, cfg!(windows)).first()
    {
        fs::create_dir_all(binary.parent().unwrap()).expect("create native Claude fixture");
        fs::write(binary, "native Claude fixture").expect("write native Claude fixture");
    }
}

fn adapter() -> &'static super::embedded::EmbeddedAdapter {
    ManagedAcpRuntime::Claude
        .embedded()
        .expect("Claude is embedded")
}

fn version_root(storage: &Path) -> PathBuf {
    storage
        .join("agent-runtimes/claude-agent-acp")
        .join(&ManagedAcpRuntime::Claude.package().runtime_id)
}

fn events(capture: &crate::logging::TestLogCapture) -> Vec<serde_json::Value> {
    capture.snapshot()
}

fn has_event(entries: &[serde_json::Value], event: &str, field: &str, value: &str) -> bool {
    entries.iter().any(|entry| {
        entry["event"] == event
            && entry["fields"][field] == value
            && entry["fields"]["agent_id"] == "claude-code"
    })
}

#[test]
fn embedded_adapter_is_the_commit_the_runtime_lockfile_pins() {
    let policy: serde_json::Value = serde_json::from_str(CLAUDE_RUNTIME_PACKAGE).unwrap();
    let adapter = adapter();
    assert_eq!(policy["openaideAdapter"]["name"], adapter.name);
    assert_eq!(policy["openaideAdapter"]["version"], adapter.version);
    assert_eq!(policy["openaideAdapter"]["commit"], adapter.commit);
    assert!(policy["dependencies"].get(adapter.name).is_none());
    for file in adapter.files {
        let digest: String = Sha256::digest(file.bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(digest, file.sha256, "{}", file.path);
    }
    assert!(adapter
        .files
        .iter()
        .any(|file| file.path == "dist/index.js"));
    assert!(adapter.files.iter().any(|file| file.path == "package.json"));
}

#[test]
fn runtime_directory_is_named_by_adapter_version_and_fork_commit() {
    let adapter = adapter();
    let package = ManagedAcpRuntime::Claude.package();
    let (version, commit) = package
        .runtime_id
        .split_once('-')
        .expect("<version>-<commit>");

    assert_eq!(version, adapter.version);
    assert_eq!(commit.len(), 12);
    assert!(adapter.commit.starts_with(commit));
    assert_eq!(
        ManagedAcpRuntime::Claude.declared_spec(),
        format!("{}@{}+{commit}", adapter.name, adapter.version)
    );
}

#[test]
fn claude_launch_installs_dependencies_unpacks_the_embedded_adapter_and_runs_it_with_node() {
    let storage = TempDir::new().expect("temporary storage root");
    let statuses = AgentStatusCache::default();
    let capture = capture_test_logs();
    let installer = RecordingInstaller::default();
    let provisioner = ManagedAcpProvisioner::with_installer_and_statuses(
        storage.path().to_path_buf(),
        Arc::new(installer.clone()),
        statuses.clone(),
    );
    assert!(!provisioner.is_provisioned(ManagedAcpRuntime::Claude));

    let launch = provisioner
        .prepare(AcpAgentConfig::claude_code())
        .expect("managed Claude launch");
    let cached = provisioner
        .prepare(AcpAgentConfig::claude_code())
        .expect("cached managed Claude launch");

    let version_root = version_root(storage.path());
    assert_eq!(installer.destinations.lock().unwrap().len(), 1);
    assert_eq!(launch.config.args, cached.config.args);
    assert!(Path::new(&launch.config.args[0]).starts_with(&version_root));
    assert!(
        launch.config.args[0].ends_with("node_modules/@openaide/claude-agent-acp/dist/index.js")
    );
    assert!(!launch.config.command.contains("npx"));
    assert!(launch.lease.is_some());
    let package_root = version_root.join("node_modules/@openaide/claude-agent-acp");
    for file in adapter().files {
        assert_eq!(
            fs::read(package_root.join(file.path)).unwrap(),
            file.bytes,
            "{}",
            file.path
        );
    }
    assert!(provisioner.is_provisioned(ManagedAcpRuntime::Claude));
    // Each runtime owns its directory, so Claude never provisions Codex.
    assert!(!provisioner.is_provisioned(ManagedAcpRuntime::Codex));
    assert_eq!(
        statuses.snapshot("claude-code").status,
        AgentStatus::Launching
    );
    let entries = events(&capture);
    for (event, field, value) in [
        (
            "claude_acp_provision_adapter_unpack_completed",
            "outcome_kind",
            "unpacked",
        ),
        (
            "claude_acp_provision_adapter_validate_started",
            "stage",
            "staged",
        ),
        (
            "claude_acp_provision_adapter_validate_completed",
            "stage",
            "staged",
        ),
        (
            "claude_acp_provision_adapter_validate_completed",
            "stage",
            "cached",
        ),
        (
            "claude_acp_provision_completed",
            "outcome_kind",
            "installed",
        ),
    ] {
        assert!(
            has_event(&entries, event, field, value),
            "{event} {field}={value}"
        );
    }
    assert!(entries
        .iter()
        .any(|entry| entry["event"] == "claude_acp_provision_started"));
}

/// Removes a launch's lease so the provisioner may repair its installation.
fn release(launch: super::PreparedManagedAcpLaunch) {
    FileExt::unlock(launch.lease.as_ref().unwrap().as_ref()).unwrap();
}

#[test]
fn modified_or_missing_adapter_files_fail_launch_validation_and_are_reinstalled() {
    for (fault, damage) in [
        (
            "digest_mismatch",
            (|path: &Path| fs::write(path, "tampered").unwrap()) as fn(&Path),
        ),
        ("missing_file", |path: &Path| fs::remove_file(path).unwrap()),
    ] {
        let storage = TempDir::new().unwrap();
        let installer = RecordingInstaller::default();
        let provisioner = ManagedAcpProvisioner::with_installer(
            storage.path().to_path_buf(),
            Arc::new(installer.clone()),
        );
        let first = provisioner.prepare(AcpAgentConfig::claude_code()).unwrap();
        let entrypoint = PathBuf::from(&first.config.args[0]);
        release(first);
        damage(&entrypoint);
        assert!(!provisioner.is_provisioned(ManagedAcpRuntime::Claude));

        let capture = capture_test_logs();
        let repaired = provisioner.prepare(AcpAgentConfig::claude_code()).unwrap();

        assert_eq!(installer.destinations.lock().unwrap().len(), 2, "{fault}");
        let index = adapter()
            .files
            .iter()
            .find(|file| file.path == "dist/index.js")
            .unwrap();
        assert_eq!(fs::read(&entrypoint).unwrap(), index.bytes);
        let entries = events(&capture);
        assert!(
            entries.iter().any(|entry| {
                entry["event"] == "claude_acp_provision_adapter_validate_completed"
                    && entry["fields"]["stage"] == "cached"
                    && entry["fields"]["outcome_kind"] == "invalid"
                    && entry["fields"]["error_kind"] == fault
            }),
            "{fault}"
        );
        release(repaired);
    }
}

#[test]
fn claude_installation_reports_installing_while_npm_runs() {
    let storage = TempDir::new().expect("temporary storage root");
    let statuses = AgentStatusCache::default();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let provisioner = ManagedAcpProvisioner::with_installer_and_statuses(
        storage.path().to_path_buf(),
        Arc::new(BlockingInstaller {
            started: started_tx,
            release: Mutex::new(release_rx),
            recording: RecordingInstaller::default(),
        }),
        statuses.clone(),
    );

    let worker = std::thread::spawn(move || provisioner.prepare(AcpAgentConfig::claude_code()));
    started_rx.recv().expect("installation should start");
    assert_eq!(
        statuses.snapshot("claude-code").status,
        AgentStatus::Installing
    );
    assert_ne!(statuses.snapshot("codex").status, AgentStatus::Installing);

    release_tx.send(()).expect("finish installation");
    worker
        .join()
        .expect("provisioning thread")
        .expect("managed Claude launch");
}

struct MissingNativeClaudeInstaller;

impl ManagedAcpInstaller for MissingNativeClaudeInstaller {
    fn install(&self, _runtime: ManagedAcpRuntime, destination: &Path) -> Result<(), String> {
        write_claude_dependencies(destination);
        let _ = fs::remove_dir_all(destination.join("node_modules/@anthropic-ai"));
        Ok(())
    }
}

#[test]
fn claude_installation_without_its_native_runtime_is_not_published() {
    if super::runtime::claude_native_binaries(Path::new("."), cfg!(windows)).is_empty() {
        // The SDK publishes no native binary for this host, so none is required.
        return;
    }
    let storage = TempDir::new().unwrap();
    let provisioner = ManagedAcpProvisioner::with_installer(
        storage.path().to_path_buf(),
        Arc::new(MissingNativeClaudeInstaller),
    );

    let error = provisioner
        .prepare(AcpAgentConfig::claude_code())
        .expect_err("a missing native runtime must fail installation");

    assert!(error
        .to_string()
        .contains("Claude integration setup failed"));
    assert!(!provisioner.is_provisioned(ManagedAcpRuntime::Claude));
    assert!(!version_root(storage.path()).exists());
}

#[test]
fn claude_installer_locks_only_the_adapter_dependencies_without_codex_patch_assets() {
    let destination = TempDir::new().unwrap();
    super::write_installer_manifest(ManagedAcpRuntime::Claude, destination.path()).unwrap();

    let package: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(destination.path().join("package.json")).unwrap())
            .unwrap();
    let lock: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(destination.path().join("package-lock.json")).unwrap(),
    )
    .unwrap();
    let adapter = adapter();
    assert_eq!(
        lock["packages"][""]["dependencies"],
        package["dependencies"]
    );
    assert!(lock["packages"][""]["dependencies"]
        .get(adapter.name)
        .is_none());
    assert!(lock["packages"]
        .get(format!("node_modules/{}", adapter.name))
        .is_none());
    assert!(lock["packages"]["node_modules/@anthropic-ai/claude-agent-sdk"]["version"].is_string());
    assert!(!destination
        .path()
        .join("apply-session-recovery.mjs")
        .exists());
}

/// Installations of one adapter version from different fork commits carry no
/// order in their names, so the newest published one is kept as the previous.
#[cfg(not(windows))]
#[test]
fn pruning_keeps_the_most_recently_installed_build_of_the_same_version() {
    let storage = TempDir::new().unwrap();
    let runtime_root = storage.path().join("agent-runtimes/claude-agent-acp");
    let installed = |name: &str, seconds: u64| {
        let root = runtime_root.join(name);
        fs::create_dir_all(&root).unwrap();
        let marker = fs::File::create(root.join(super::MANAGED_MARKER)).unwrap();
        // timing: data — publication times order installations; nothing waits on them.
        let published = std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds);
        marker.set_modified(published).unwrap();
    };
    installed("1.0.0-bbbbbbbbbbbb", 1_000);
    installed("1.0.0-aaaaaaaaaaaa", 2_000);
    let provisioner = ManagedAcpProvisioner::with_installer(
        storage.path().to_path_buf(),
        Arc::new(RecordingInstaller::default()),
    );

    let launch = provisioner.prepare(AcpAgentConfig::claude_code()).unwrap();

    assert!(runtime_root.join("1.0.0-aaaaaaaaaaaa").exists());
    assert!(!runtime_root.join("1.0.0-bbbbbbbbbbbb").exists());
    release(launch);
}
