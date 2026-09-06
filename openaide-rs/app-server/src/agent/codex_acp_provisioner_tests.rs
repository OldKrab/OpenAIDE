use std::fs;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use fs2::FileExt;
use tempfile::TempDir;

use super::{process_path_argument, CodexAcpInstaller, CodexAcpProvisioner};
use crate::agent::acp_agent_config::AcpAgentConfig;
use crate::agent::status_cache::AgentStatusCache;
use crate::logging::capture_test_logs;
use openaide_app_server_protocol::snapshot::AgentStatus;

pub(super) const INDEX_FIXTURE: &[u8] = b"#!/usr/bin/env node\n";
pub(super) const HELPER_FIXTURE: &[u8] = b"export const fixture = true;\n";

#[derive(Clone, Default)]
struct RecordingInstaller {
    destinations: Arc<Mutex<Vec<std::path::PathBuf>>>,
}

#[test]
fn windows_lock_violation_is_retryable_contention() {
    let error = std::io::Error::from_raw_os_error(33);

    assert!(super::lock_error_is_contention(&error, true));
}

#[test]
fn explicit_codex_launch_times_out_while_another_process_owns_provisioning() {
    let storage = TempDir::new().expect("temporary storage root");
    let runtime_root = storage.path().join("agent-runtimes/codex-acp");
    fs::create_dir_all(&runtime_root).expect("create managed runtime root");
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(runtime_root.join(".install.lock"))
        .expect("open install lock");
    lock.lock_exclusive().expect("hold install lock");
    let provisioner = CodexAcpProvisioner::with_installer_and_timeout(
        storage.path().to_path_buf(),
        Arc::new(RecordingInstaller::default()),
        Duration::from_millis(20),
    );

    let error = provisioner
        .prepare(AcpAgentConfig::codex())
        .expect_err("contended installation should time out");

    assert!(matches!(
        error,
        crate::protocol::errors::RuntimeError::NotReady(_)
    ));
    FileExt::unlock(&lock).expect("release install lock");
}

impl CodexAcpInstaller for RecordingInstaller {
    fn install(&self, destination: &std::path::Path) -> Result<(), String> {
        self.destinations
            .lock()
            .expect("installer destinations poisoned")
            .push(destination.to_path_buf());
        let package_root = destination.join("node_modules/@openaide/codex-acp");
        fs::create_dir_all(package_root.join("dist")).expect("create managed package fixture");
        fs::write(
            package_root.join("package.json"),
            r#"{"name":"@openaide/codex-acp","version":"1.2.0"}"#,
        )
        .expect("write managed package manifest");
        fs::write(package_root.join("dist/index.js"), INDEX_FIXTURE)
            .expect("write managed package entrypoint");
        fs::write(
            package_root.join("dist/openaide-session-recovery.mjs"),
            HELPER_FIXTURE,
        )
        .expect("write managed recovery helper");
        let codex_root = destination
            .join("node_modules/@openai/codex-win32-x64/vendor/x86_64-pc-windows-msvc/bin");
        fs::create_dir_all(&codex_root).expect("create managed native Codex fixture");
        fs::write(codex_root.join("codex.exe"), "native Codex fixture")
            .expect("write managed native Codex fixture");
        Ok(())
    }
}

struct BlockingInstaller {
    started: std::sync::mpsc::Sender<()>,
    release: Mutex<std::sync::mpsc::Receiver<()>>,
    recording: RecordingInstaller,
}

impl CodexAcpInstaller for BlockingInstaller {
    fn install(&self, destination: &std::path::Path) -> Result<(), String> {
        self.started.send(()).expect("report installation start");
        self.release
            .lock()
            .expect("installer release poisoned")
            .recv()
            .expect("release installation");
        self.recording.install(destination)
    }
}

#[test]
fn concurrent_explicit_codex_launches_wait_for_one_shared_installation() {
    let storage = TempDir::new().expect("temporary storage root");
    let recording = RecordingInstaller::default();
    let (install_started_tx, install_started_rx) = std::sync::mpsc::channel();
    let (release_install_tx, release_install_rx) = std::sync::mpsc::channel();
    let provisioner = Arc::new(CodexAcpProvisioner::with_installer(
        storage.path().to_path_buf(),
        Arc::new(BlockingInstaller {
            started: install_started_tx,
            release: Mutex::new(release_install_rx),
            recording: recording.clone(),
        }),
    ));

    let owner_provisioner = provisioner.clone();
    let owner = std::thread::spawn(move || owner_provisioner.prepare(AcpAgentConfig::codex()));
    install_started_rx
        .recv()
        .expect("first launch should own installation");

    let waiter_provisioner = provisioner.clone();
    let (waiter_completed_tx, waiter_completed_rx) = std::sync::mpsc::channel();
    let waiter = std::thread::spawn(move || {
        waiter_completed_tx
            .send(waiter_provisioner.prepare(AcpAgentConfig::codex()))
            .expect("report waiting launch result");
    });
    assert!(matches!(
        waiter_completed_rx.recv_timeout(Duration::from_millis(50)),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout)
    ));

    release_install_tx
        .send(())
        .expect("finish shared installation");
    owner
        .join()
        .expect("installation owner thread")
        .expect("installation owner launch");
    waiter_completed_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("waiting launch should finish after installation")
        .expect("waiting launch should reuse installation");
    waiter.join().expect("waiting launch thread");
    assert_eq!(
        recording
            .destinations
            .lock()
            .expect("installer destinations poisoned")
            .len(),
        1,
    );
}

#[test]
fn installation_publishes_one_authoritative_agent_activity() {
    let storage = TempDir::new().expect("temporary storage root");
    let statuses = AgentStatusCache::default();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let provisioner = CodexAcpProvisioner::with_installer_and_statuses(
        storage.path().to_path_buf(),
        Arc::new(BlockingInstaller {
            started: started_tx,
            release: Mutex::new(release_rx),
            recording: RecordingInstaller::default(),
        }),
        statuses.clone(),
    );

    let worker = std::thread::spawn(move || provisioner.prepare(AcpAgentConfig::codex()));
    started_rx.recv().expect("installation should start");
    assert_eq!(statuses.snapshot("codex").status, AgentStatus::Installing);

    release_tx.send(()).expect("finish installation");
    worker
        .join()
        .expect("provisioning thread")
        .expect("managed launch");
    assert_eq!(statuses.snapshot("codex").status, AgentStatus::Launching);
}

#[test]
fn explicit_codex_launch_installs_the_locked_integration_once_and_reuses_it() {
    let storage = TempDir::new().expect("temporary storage root");
    let installer = RecordingInstaller::default();
    let provisioner = CodexAcpProvisioner::with_installer(
        storage.path().to_path_buf(),
        Arc::new(installer.clone()),
    );

    let first = provisioner
        .prepare(AcpAgentConfig::codex())
        .expect("first managed Codex launch");
    let second = provisioner
        .prepare(AcpAgentConfig::codex())
        .expect("cached managed Codex launch");

    assert_eq!(
        installer
            .destinations
            .lock()
            .expect("installer destinations poisoned")
            .len(),
        1,
    );
    assert_eq!(first.config.command, second.config.command);
    assert_eq!(first.config.args, second.config.args);
    assert_eq!(first.config.args.len(), 1);
    assert!(std::path::Path::new(&first.config.args[0]).is_absolute());
    assert!(first.config.args[0].ends_with("node_modules/@openaide/codex-acp/dist/index.js"));
    assert!(!first.config.command.contains("npx"));
    assert!(!first
        .config
        .args
        .iter()
        .any(|argument| argument.contains("agentclientprotocol")));
}

#[test]
fn patched_runtime_uses_a_separate_cache_and_preserves_the_leased_upstream_runtime() {
    let storage = TempDir::new().expect("temporary storage root");
    let runtime_root = storage.path().join("agent-runtimes/codex-acp");
    let upstream = runtime_root.join("1.2.0");
    RecordingInstaller::default().install(&upstream).unwrap();
    fs::write(
        upstream.join(".openaide-managed"),
        "@openaide/codex-acp@1.2.0\n",
    )
    .unwrap();
    let original =
        fs::read(upstream.join("node_modules/@openaide/codex-acp/dist/index.js")).unwrap();
    let lease = super::open_lock_file(&upstream.join(".lease")).unwrap();
    FileExt::lock_shared(&lease).unwrap();
    // Retaining only the newest previous runtime must still protect an older
    // running owner's lease when a new managed patch cache is installed.
    fs::create_dir_all(runtime_root.join("1.2.1")).unwrap();
    let installer = RecordingInstaller::default();
    let provisioner = CodexAcpProvisioner::with_installer(
        storage.path().to_path_buf(),
        Arc::new(installer.clone()),
    );

    let launch = provisioner.prepare(AcpAgentConfig::codex()).unwrap();

    assert!(std::path::Path::new(&launch.config.args[0])
        .starts_with(runtime_root.join("1.2.0-openaide.2")));
    assert_eq!(installer.destinations.lock().unwrap().len(), 1);
    assert_eq!(
        fs::read(upstream.join("node_modules/@openaide/codex-acp/dist/index.js")).unwrap(),
        original
    );
    assert!(upstream.join(".lease").is_file());
}

#[test]
fn changed_adapter_or_recovery_helper_is_reinstalled_before_another_launch() {
    for artifact in ["index.js", "openaide-session-recovery.mjs"] {
        let storage = TempDir::new().unwrap();
        let installer = RecordingInstaller::default();
        let provisioner = CodexAcpProvisioner::with_installer(
            storage.path().to_path_buf(),
            Arc::new(installer.clone()),
        );
        let first = provisioner.prepare(AcpAgentConfig::codex()).unwrap();
        let path = std::path::Path::new(&first.config.args[0])
            .parent()
            .unwrap()
            .join(artifact);
        // Release the fixture owner explicitly: a concurrently spawning test
        // can briefly inherit an open flock before close-on-exec takes effect.
        FileExt::unlock(first.lease.as_ref().unwrap().as_ref()).unwrap();
        drop(first);
        fs::write(path, "incomplete or unpatched runtime").unwrap();

        assert!(
            !provisioner.is_provisioned(),
            "changed {artifact} must invalidate the cache"
        );
        let _repaired = provisioner.prepare(AcpAgentConfig::codex()).unwrap();
        assert!(provisioner.is_provisioned());
        assert_eq!(installer.destinations.lock().unwrap().len(), 2);
    }
}

#[test]
fn invalid_current_runtime_is_preserved_until_its_running_owner_releases_the_lease() {
    let storage = TempDir::new().unwrap();
    let installer = RecordingInstaller::default();
    let provisioner = CodexAcpProvisioner::with_installer(
        storage.path().to_path_buf(),
        Arc::new(installer.clone()),
    );
    let running = provisioner.prepare(AcpAgentConfig::codex()).unwrap();
    let entrypoint = std::path::PathBuf::from(&running.config.args[0]);
    fs::write(&entrypoint, "changed while an owner still holds its lease").unwrap();

    let error = provisioner
        .prepare(AcpAgentConfig::codex())
        .expect_err("repair cannot remove a live runtime");
    assert!(error.to_string().contains("in use"));
    assert_eq!(
        fs::read_to_string(&entrypoint).unwrap(),
        "changed while an owner still holds its lease"
    );
    assert_eq!(installer.destinations.lock().unwrap().len(), 1);
    FileExt::unlock(running.lease.as_ref().unwrap().as_ref()).unwrap();
    drop(running);

    let _repaired = provisioner.prepare(AcpAgentConfig::codex()).unwrap();
    assert_eq!(fs::read(&entrypoint).unwrap(), INDEX_FIXTURE);
    assert_eq!(installer.destinations.lock().unwrap().len(), 2);
}

struct IncompletePatchInstaller {
    failure: &'static str,
}

impl CodexAcpInstaller for IncompletePatchInstaller {
    fn install(&self, destination: &std::path::Path) -> Result<(), String> {
        RecordingInstaller::default().install(destination)?;
        let dist = destination.join("node_modules/@openaide/codex-acp/dist");
        match self.failure {
            "installer_failure" => Err("managed patch installation failed".to_string()),
            "unpatched_adapter" => fs::write(dist.join("index.js"), "original upstream adapter")
                .map_err(|error| error.to_string()),
            "missing_helper" => fs::remove_file(dist.join("openaide-session-recovery.mjs"))
                .map_err(|error| error.to_string()),
            _ => unreachable!(),
        }
    }
}

#[test]
fn failed_or_incomplete_patch_is_never_published_as_a_managed_runtime() {
    for failure in ["installer_failure", "unpatched_adapter", "missing_helper"] {
        let storage = TempDir::new().unwrap();
        let provisioner = CodexAcpProvisioner::with_installer(
            storage.path().to_path_buf(),
            Arc::new(IncompletePatchInstaller { failure }),
        );
        let error = provisioner
            .prepare(AcpAgentConfig::codex())
            .expect_err("all managed artifacts must validate before publication");
        assert!(matches!(
            error,
            crate::protocol::errors::RuntimeError::NotReady(_)
        ));
        assert!(!provisioner.is_provisioned());
        let runtime_root = storage.path().join("agent-runtimes/codex-acp");
        assert!(!runtime_root
            .join(&super::runtime_manifest().runtime_id)
            .exists());
        assert!(fs::read_dir(&runtime_root).unwrap().all(|entry| !entry
            .unwrap()
            .file_type()
            .unwrap()
            .is_dir()));

        let retry = CodexAcpProvisioner::with_installer(
            storage.path().to_path_buf(),
            Arc::new(RecordingInstaller::default()),
        );
        let _launch = retry.prepare(AcpAgentConfig::codex()).unwrap();
        assert!(retry.is_provisioned());
    }
}

struct NodePatchInstaller;

impl CodexAcpInstaller for NodePatchInstaller {
    fn install(&self, destination: &std::path::Path) -> Result<(), String> {
        RecordingInstaller::default().install(destination)?;
        let dist = destination.join("node_modules/@openaide/codex-acp/dist");
        fs::write(dist.join("index.js"), "unpatched input").unwrap();
        fs::remove_file(dist.join("openaide-session-recovery.mjs")).unwrap();
        // A real Node stage process must finish both artifacts before Rust can
        // validate and publish the runtime. This fixture needs no npm/network.
        fs::write(
            destination.join("apply-session-recovery.mjs"),
            r#"
import { writeFile } from 'node:fs/promises';
import path from 'node:path';
const dist = path.join(process.argv[2], 'node_modules/@openaide/codex-acp/dist');
await writeFile(path.join(dist, 'index.js'), '#!/usr/bin/env node\n');
await writeFile(path.join(dist, 'openaide-session-recovery.mjs'), 'export const fixture = true;\n');
"#,
        )
        .unwrap();
        super::runtime_patch::apply(
            destination,
            std::time::Instant::now() + Duration::from_secs(5),
        )
    }
}

#[test]
fn node_patch_finishes_in_staging_before_a_launch_can_use_its_artifacts() {
    let storage = TempDir::new().unwrap();
    let provisioner = CodexAcpProvisioner::with_installer(
        storage.path().to_path_buf(),
        Arc::new(NodePatchInstaller),
    );
    let launch = provisioner.prepare(AcpAgentConfig::codex()).unwrap();
    assert_eq!(fs::read(&launch.config.args[0]).unwrap(), INDEX_FIXTURE);
    assert!(provisioner.is_provisioned());
}

#[test]
fn windows_launch_uses_the_managed_native_codex_binary() {
    let storage = TempDir::new().expect("temporary storage root");
    let provisioner = CodexAcpProvisioner::with_installer_for_platform(
        storage.path().to_path_buf(),
        Arc::new(RecordingInstaller::default()),
        true,
    );

    let launch = provisioner
        .prepare(AcpAgentConfig::codex())
        .expect("managed Windows Codex launch");

    let expected_codex = storage
        .path()
        .join("agent-runtimes/codex-acp")
        .join(&super::runtime_manifest().runtime_id)
        .join("node_modules/@openai/codex-win32-x64")
        .join("vendor/x86_64-pc-windows-msvc/bin/codex.exe");
    assert_eq!(
        launch
            .config
            .env
            .iter()
            .find(|(name, _)| name == "CODEX_PATH")
            .map(|(_, value)| value.as_str()),
        Some(process_path_argument(&expected_codex, true).as_str()),
    );
    assert_eq!(launch.config.args.len(), 1);
    assert!(launch.config.args[0].ends_with("node_modules/@openaide/codex-acp/dist/index.js"));
    assert!(!launch.config.command.ends_with(".cmd"));
}

#[test]
fn windows_process_paths_are_unambiguous_to_node() {
    let path =
        std::path::Path::new(r"\\?\C:\Users\runneradmin\agent-runtimes\codex-acp\dist\index.js");

    assert_eq!(
        process_path_argument(path, true),
        "C:/Users/runneradmin/agent-runtimes/codex-acp/dist/index.js"
    );
    assert_eq!(
        process_path_argument(
            std::path::Path::new(r"\\?\UNC\server\share\codex-acp\dist\index.js"),
            true,
        ),
        "//server/share/codex-acp/dist/index.js"
    );
}

#[test]
fn passive_discovery_can_detect_an_unprovisioned_integration_without_installing_it() {
    let storage = TempDir::new().expect("temporary storage root");
    let installer = RecordingInstaller::default();
    let provisioner = CodexAcpProvisioner::with_installer(
        storage.path().to_path_buf(),
        Arc::new(installer.clone()),
    );

    assert!(!provisioner.is_provisioned());
    assert!(installer
        .destinations
        .lock()
        .expect("installer destinations poisoned")
        .is_empty());
}

#[test]
fn installation_logs_a_safe_start_and_terminal_event() {
    let storage = TempDir::new().expect("temporary storage root");
    let capture = capture_test_logs();
    let provisioner = CodexAcpProvisioner::with_installer(
        storage.path().to_path_buf(),
        Arc::new(RecordingInstaller::default()),
    );

    provisioner
        .prepare(AcpAgentConfig::codex())
        .expect("managed Codex launch");

    let logs = capture.snapshot();
    let events = logs
        .iter()
        .filter_map(|entry| entry.get("event").and_then(serde_json::Value::as_str))
        .collect::<Vec<_>>();
    assert!(events.contains(&"codex_acp_provision_started"));
    assert!(events.contains(&"codex_acp_provision_completed"));
    let serialized = serde_json::to_string(&logs).expect("serialize captured logs");
    assert!(!serialized.contains(storage.path().to_string_lossy().as_ref()));
    assert!(!serialized.contains("node_modules"));
}

struct MissingNodeInstaller;

impl CodexAcpInstaller for MissingNodeInstaller {
    fn install(&self, _destination: &std::path::Path) -> Result<(), String> {
        Err("npm could not be started".to_string())
    }
}

#[test]
fn missing_npm_is_reported_as_a_node_js_setup_requirement() {
    let storage = TempDir::new().expect("temporary storage root");
    let provisioner = CodexAcpProvisioner::with_installer(
        storage.path().to_path_buf(),
        Arc::new(MissingNodeInstaller),
    );

    let error = provisioner
        .prepare(AcpAgentConfig::codex())
        .expect_err("npm should be required for the initial install");

    assert!(matches!(
        error,
        crate::protocol::errors::RuntimeError::NodeJsRequired(_)
    ));
}

#[test]
fn successful_install_removes_stale_staging_and_unleased_versions_beyond_previous() {
    let storage = TempDir::new().expect("temporary storage root");
    let runtime_root = storage.path().join("agent-runtimes/codex-acp");
    fs::create_dir_all(runtime_root.join(".1.1.5.installing-abandoned"))
        .expect("create stale staging fixture");
    for version in ["1.0.0", "1.0.1", "1.1.0"] {
        fs::create_dir_all(runtime_root.join(version)).expect("create old version fixture");
    }
    let provisioner = CodexAcpProvisioner::with_installer(
        storage.path().to_path_buf(),
        Arc::new(RecordingInstaller::default()),
    );

    provisioner
        .prepare(AcpAgentConfig::codex())
        .expect("managed Codex launch");

    assert!(!runtime_root.join(".1.1.5.installing-abandoned").exists());
    assert!(runtime_root.join("1.1.0").exists());
    assert!(!runtime_root.join("1.0.1").exists());
    assert!(!runtime_root.join("1.0.0").exists());
}
