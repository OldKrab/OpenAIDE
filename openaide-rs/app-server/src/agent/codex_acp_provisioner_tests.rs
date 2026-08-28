use std::fs;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use fs2::FileExt;
use tempfile::TempDir;

use super::{process_path_argument, CodexAcpInstaller, CodexAcpProvisioner, CODEX_ACP_VERSION};
use crate::agent::acp_agent_config::AcpAgentConfig;
use crate::agent::status_cache::AgentStatusCache;
use crate::logging::capture_test_logs;
use openaide_app_server_protocol::snapshot::AgentStatus;

#[derive(Clone, Default)]
struct RecordingInstaller {
    destinations: Arc<Mutex<Vec<std::path::PathBuf>>>,
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
            format!(r#"{{"name":"@openaide/codex-acp","version":"{CODEX_ACP_VERSION}"}}"#),
        )
        .expect("write managed package manifest");
        fs::write(package_root.join("dist/index.js"), "#!/usr/bin/env node\n")
            .expect("write managed package entrypoint");
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
}

impl CodexAcpInstaller for BlockingInstaller {
    fn install(&self, destination: &std::path::Path) -> Result<(), String> {
        self.started.send(()).expect("report installation start");
        self.release
            .lock()
            .expect("installer release poisoned")
            .recv()
            .expect("release installation");
        RecordingInstaller::default().install(destination)
    }
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
        .join(CODEX_ACP_VERSION)
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
    let path = std::path::Path::new(r"C:\Users\runneradmin\agent-runtimes\codex-acp\dist\index.js");

    assert_eq!(
        process_path_argument(path, true),
        "C:/Users/runneradmin/agent-runtimes/codex-acp/dist/index.js"
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
