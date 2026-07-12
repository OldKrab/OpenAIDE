use super::*;
use crate::app_server_client::EndpointProbeOutcome;
use crate::storage_runtime::{
    RuntimeEndpoint, RuntimeEndpointRecord, RuntimeEndpointRecordStatus, StateRoot, TransportKind,
};

#[test]
fn compatible_endpoint_is_probed_and_returned_for_attach() {
    let fixture = Fixture::new();
    let record = endpoint_record(fixture.state_root.fingerprint(), "server-1");
    fixture
        .endpoint_records
        .write(fixture.state_root.fingerprint(), &record)
        .unwrap();
    let mut prober = FixedProber::new(EndpointProbeOutcome::Compatible);

    let result = fixture.run(&mut prober);

    assert!(matches!(
        result.unwrap(),
        AttachOrLaunchRunResult::AttachExisting { target }
            if target.server_id == "server-1"
    ));
    assert_eq!(prober.calls, 1);
}

#[test]
fn missing_endpoint_with_launch_lock_returns_launch_handoff() {
    let fixture = Fixture::new();
    let mut prober = FixedProber::new(EndpointProbeOutcome::Compatible);

    let result = fixture.run(&mut prober);

    assert!(matches!(
        result.unwrap(),
        AttachOrLaunchRunResult::LaunchNew { .. }
    ));
    assert_eq!(prober.calls, 0);
}

#[test]
fn busy_launch_lock_returns_wait_handoff() {
    let fixture = Fixture::new();
    let held_lock = RuntimeLock::acquire(&fixture.launch_lock_path).unwrap();
    assert!(matches!(held_lock, LockAcquireOutcome::Acquired(_)));
    let mut prober = FixedProber::new(EndpointProbeOutcome::Compatible);

    let result = fixture.run(&mut prober);

    assert!(matches!(
        result.unwrap(),
        AttachOrLaunchRunResult::WaitForLaunch { lock_path }
            if lock_path == fixture.launch_lock_path
    ));
}

#[test]
fn unreachable_endpoint_is_removed_then_launch_is_elected() {
    let fixture = Fixture::new();
    fixture
        .endpoint_records
        .write(
            fixture.state_root.fingerprint(),
            &endpoint_record(fixture.state_root.fingerprint(), "server-1"),
        )
        .unwrap();
    let mut prober = FixedProber::new(EndpointProbeOutcome::Unreachable);

    let result = fixture.run(&mut prober);

    assert!(matches!(
        result.unwrap(),
        AttachOrLaunchRunResult::LaunchNew { .. }
    ));
    assert_eq!(
        fixture
            .endpoint_records
            .read(fixture.state_root.fingerprint())
            .unwrap(),
        None
    );
}

#[test]
fn storage_writer_block_returns_typed_failure_without_probe() {
    let fixture = Fixture::new();
    fixture
        .endpoint_records
        .write(
            fixture.state_root.fingerprint(),
            &endpoint_record(fixture.state_root.fingerprint(), "server-1"),
        )
        .unwrap();
    let mut prober = FixedProber::new(EndpointProbeOutcome::Compatible);

    let result = fixture.run_with_storage(StorageWriterState::Blocked, &mut prober);

    assert!(matches!(
        result.unwrap(),
        AttachOrLaunchRunResult::Fail {
            reason: AttachOrLaunchFailure::StorageStateRootBlocked
        }
    ));
    assert_eq!(prober.calls, 0);
    assert!(!fixture.launch_lock_path.exists());
}

#[test]
fn endpoint_removed_during_probe_falls_back_to_launch_lock_path() {
    let fixture = Fixture::new();
    fixture
        .endpoint_records
        .write(
            fixture.state_root.fingerprint(),
            &endpoint_record(fixture.state_root.fingerprint(), "server-1"),
        )
        .unwrap();
    let mut prober = RemovingProber {
        endpoint_records: fixture.endpoint_records.clone(),
        fingerprint: fixture.state_root.fingerprint().clone(),
    };

    let result = fixture.run(&mut prober);

    assert!(matches!(
        result.unwrap(),
        AttachOrLaunchRunResult::LaunchNew { .. }
    ));
}

#[test]
fn endpoint_replaced_during_probe_retries_current_record_without_mismatch() {
    let fixture = Fixture::new();
    fixture
        .endpoint_records
        .write(
            fixture.state_root.fingerprint(),
            &endpoint_record(fixture.state_root.fingerprint(), "server-1"),
        )
        .unwrap();
    let mut prober = ReplacingProber {
        endpoint_records: fixture.endpoint_records.clone(),
        fingerprint: fixture.state_root.fingerprint().clone(),
        replacement: endpoint_record(fixture.state_root.fingerprint(), "server-2"),
        calls: 0,
    };

    let result = fixture.run(&mut prober);

    assert!(matches!(
        result.unwrap(),
        AttachOrLaunchRunResult::AttachExisting { target }
            if target.server_id == "server-2"
    ));
    assert_eq!(prober.calls, 2);
}

#[test]
fn stale_cleanup_does_not_remove_replaced_endpoint_record() {
    let fixture = Fixture::new();
    fixture
        .endpoint_records
        .write(
            fixture.state_root.fingerprint(),
            &endpoint_record(fixture.state_root.fingerprint(), "server-1"),
        )
        .unwrap();
    let replacement = endpoint_record(fixture.state_root.fingerprint(), "server-2");
    let mut prober = ReplacingThenUnreachableProber {
        endpoint_records: fixture.endpoint_records.clone(),
        fingerprint: fixture.state_root.fingerprint().clone(),
        replacement: replacement.clone(),
        calls: 0,
    };

    let result = fixture.run(&mut prober);

    assert!(matches!(
        result.unwrap(),
        AttachOrLaunchRunResult::AttachExisting { target }
            if target.server_id == "server-2"
    ));
    assert_eq!(
        fixture
            .endpoint_records
            .read(fixture.state_root.fingerprint())
            .unwrap(),
        Some(replacement)
    );
}

#[test]
fn mismatched_probe_report_is_rejected() {
    let fixture = Fixture::new();
    fixture
        .endpoint_records
        .write(
            fixture.state_root.fingerprint(),
            &endpoint_record(fixture.state_root.fingerprint(), "server-1"),
        )
        .unwrap();
    let other_root = StateRoot::resolve(tempfile::tempdir().unwrap().path()).unwrap();
    let mut prober = MismatchedProber {
        record: endpoint_record(other_root.fingerprint(), "server-other"),
    };

    let result = fixture.run(&mut prober);

    assert!(matches!(
        result,
        Err(AttachOrLaunchRunError::ProbeReportMismatch)
    ));
}

struct Fixture {
    state_root: StateRoot,
    endpoint_records: EndpointRecordStore,
    launch_lock_path: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let state_dir = tempfile::tempdir().unwrap().keep();
        let runtime_dir = tempfile::tempdir().unwrap().keep();
        let state_root = StateRoot::resolve(&state_dir).unwrap();
        let launch_lock_path = runtime_dir.join("launch.lock");
        Self {
            state_root,
            endpoint_records: EndpointRecordStore::new(runtime_dir),
            launch_lock_path,
        }
    }

    fn run<P: EndpointProber>(
        &self,
        prober: &mut P,
    ) -> Result<AttachOrLaunchRunResult, AttachOrLaunchRunError> {
        self.run_with_storage(StorageWriterState::Available, prober)
    }

    fn run_with_storage<P: EndpointProber>(
        &self,
        storage_writer: StorageWriterState,
        prober: &mut P,
    ) -> Result<AttachOrLaunchRunResult, AttachOrLaunchRunError> {
        let runner =
            AttachOrLaunchRunner::new(self.endpoint_records.clone(), self.launch_lock_path.clone());
        runner.run(
            self.state_root.fingerprint(),
            &requirements(),
            storage_writer,
            prober,
        )
    }
}

struct FixedProber {
    outcome: EndpointProbeOutcome,
    calls: usize,
}

impl FixedProber {
    fn new(outcome: EndpointProbeOutcome) -> Self {
        Self { outcome, calls: 0 }
    }
}

impl EndpointProber for FixedProber {
    fn probe(
        &mut self,
        target: EndpointTarget,
        requirements: EndpointRequirements,
    ) -> Result<EndpointProbeReport, EndpointProbeError> {
        self.calls += 1;
        Ok(EndpointProbeReport {
            target,
            requirements,
            outcome: self.outcome,
        })
    }
}

struct MismatchedProber {
    record: RuntimeEndpointRecord,
}

impl EndpointProber for MismatchedProber {
    fn probe(
        &mut self,
        _target: EndpointTarget,
        requirements: EndpointRequirements,
    ) -> Result<EndpointProbeReport, EndpointProbeError> {
        Ok(EndpointProbeReport {
            target: endpoint_target(&self.record),
            requirements,
            outcome: EndpointProbeOutcome::Compatible,
        })
    }
}

struct ReplacingProber {
    endpoint_records: EndpointRecordStore,
    fingerprint: StateRootFingerprint,
    replacement: RuntimeEndpointRecord,
    calls: usize,
}

impl EndpointProber for ReplacingProber {
    fn probe(
        &mut self,
        target: EndpointTarget,
        requirements: EndpointRequirements,
    ) -> Result<EndpointProbeReport, EndpointProbeError> {
        self.calls += 1;
        if self.calls == 1 {
            self.endpoint_records
                .write(&self.fingerprint, &self.replacement)
                .unwrap();
        }
        Ok(EndpointProbeReport {
            target,
            requirements,
            outcome: EndpointProbeOutcome::Compatible,
        })
    }
}

struct ReplacingThenUnreachableProber {
    endpoint_records: EndpointRecordStore,
    fingerprint: StateRootFingerprint,
    replacement: RuntimeEndpointRecord,
    calls: usize,
}

impl EndpointProber for ReplacingThenUnreachableProber {
    fn probe(
        &mut self,
        target: EndpointTarget,
        requirements: EndpointRequirements,
    ) -> Result<EndpointProbeReport, EndpointProbeError> {
        self.calls += 1;
        let outcome = if self.calls == 1 {
            self.endpoint_records
                .write(&self.fingerprint, &self.replacement)
                .unwrap();
            EndpointProbeOutcome::Unreachable
        } else {
            EndpointProbeOutcome::Compatible
        };
        Ok(EndpointProbeReport {
            target,
            requirements,
            outcome,
        })
    }
}

struct RemovingProber {
    endpoint_records: EndpointRecordStore,
    fingerprint: StateRootFingerprint,
}

impl EndpointProber for RemovingProber {
    fn probe(
        &mut self,
        target: EndpointTarget,
        requirements: EndpointRequirements,
    ) -> Result<EndpointProbeReport, EndpointProbeError> {
        self.endpoint_records.remove(&self.fingerprint).unwrap();
        Ok(EndpointProbeReport {
            target,
            requirements,
            outcome: EndpointProbeOutcome::Compatible,
        })
    }
}

fn requirements() -> AttachOrLaunchRequirements {
    AttachOrLaunchRequirements {
        required_protocol_version: "1".to_string(),
        required_app_version: "0.1.0".to_string(),
    }
}

fn endpoint_record(fingerprint: &StateRootFingerprint, server_id: &str) -> RuntimeEndpointRecord {
    RuntimeEndpointRecord {
        server_id: server_id.to_string(),
        state_root_fingerprint: fingerprint.as_str().to_string(),
        pid: std::process::id(),
        protocol_version: "1".to_string(),
        app_version: "0.1.0".to_string(),
        status: RuntimeEndpointRecordStatus::Running,
        auth_token: "token".to_string(),
        endpoints: vec![RuntimeEndpoint {
            transport: TransportKind::LocalHttp,
            address: "http://127.0.0.1:12345".to_string(),
        }],
    }
}

fn endpoint_target(record: &RuntimeEndpointRecord) -> EndpointTarget {
    EndpointTarget {
        server_id: record.server_id.clone(),
        state_root_fingerprint: record.state_root_fingerprint.clone(),
        protocol_version: record.protocol_version.clone(),
        app_version: record.app_version.clone(),
        auth_token: record.auth_token.clone(),
        endpoints: record.endpoints.clone(),
    }
}
