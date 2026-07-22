use std::path::Path;

use openaide_app_server_protocol::client::APP_SERVER_PROTOCOL_VERSION;

use super::*;
use crate::protocol_edge::stdio::ProtocolEdgeStdioDispatcher;
use crate::storage_runtime::{EndpointRecordStore, LockAcquireOutcome, RuntimeLock, StateRoot};

#[test]
fn missing_endpoint_elects_this_process_to_launch() {
    let fixture = Fixture::new();
    let mut waiter = NoopWaiter;

    let result = fixture.run(&mut waiter).unwrap();

    assert!(matches!(result, LaunchHandoffResult::LaunchNew { .. }));
}

#[test]
fn compatible_endpoint_attaches_without_waiting() {
    let fixture = Fixture::new();
    let dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(fixture.state_root.clone());
    let _published = crate::app_server_process::publish_local_http_probe_endpoint(
        dispatcher.shared_gateway(),
        &fixture.state_root,
        fixture.runtime_dir.path(),
    )
    .unwrap();
    let mut waiter = CountingWaiter::default();

    let result = fixture.run(&mut waiter).unwrap();

    assert!(matches!(
        result,
        LaunchHandoffResult::AttachExisting { target }
            if target.state_root_fingerprint == fixture.state_root.fingerprint().as_str()
    ));
    assert_eq!(waiter.calls, 0);
}

#[test]
fn incompatible_server_with_replacement_endpoint_stops_then_launches_replacement() {
    let fixture = Fixture::new();
    let dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(fixture.state_root.clone());
    let published = crate::app_server_process::publish_local_http_probe_endpoint(
        dispatcher.shared_gateway(),
        &fixture.state_root,
        fixture.runtime_dir.path(),
    )
    .unwrap();
    let mut waiter = RemoveIncompatibleEndpointAfterWait {
        published: Some(published),
        calls: 0,
    };

    let result = fixture
        .run_requiring_app_version("next-release", &mut waiter)
        .unwrap();

    assert!(matches!(result, LaunchHandoffResult::LaunchNew { .. }));
    assert_eq!(waiter.calls, 1);
}

#[test]
fn legacy_release_without_replacement_endpoint_waits_for_liveness_shutdown() {
    let fixture = Fixture::new();
    let dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(fixture.state_root.clone());
    let published = crate::app_server_process::publish_local_http_probe_endpoint(
        dispatcher.shared_gateway(),
        &fixture.state_root,
        fixture.runtime_dir.path(),
    )
    .unwrap();
    let mut legacy_record = fixture
        .endpoint_records
        .read(fixture.state_root.fingerprint())
        .unwrap()
        .expect("published endpoint");
    legacy_record.replacement_token = None;
    fixture
        .endpoint_records
        .write(fixture.state_root.fingerprint(), &legacy_record)
        .unwrap();
    let mut waiter = RemoveIncompatibleEndpointAfterWait {
        published: Some(published),
        calls: 0,
    };

    let result = fixture
        .run_requiring_app_version("next-release", &mut waiter)
        .unwrap();

    assert!(matches!(result, LaunchHandoffResult::LaunchNew { .. }));
    assert_eq!(waiter.calls, 1);
}

#[test]
fn busy_launch_lock_waits_then_retries_endpoint_probe() {
    let fixture = Fixture::new();
    let held_lock = acquire_launch_lock(&fixture);
    let mut waiter = PublishAfterWait {
        held_lock: Some(held_lock),
        state_root: fixture.state_root.clone(),
        runtime_root: fixture.runtime_dir.path().to_path_buf(),
        published: None,
        calls: 0,
    };

    let result = fixture.run(&mut waiter).unwrap();

    assert!(matches!(
        result,
        LaunchHandoffResult::AttachExisting { target }
            if target.state_root_fingerprint == fixture.state_root.fingerprint().as_str()
    ));
    assert_eq!(waiter.calls, 1);
}

#[test]
fn busy_launch_lock_is_bounded_by_policy() {
    let fixture = Fixture::with_policy(LaunchHandoffPolicy {
        max_wait_attempts: 2,
    });
    let _held_lock = acquire_launch_lock(&fixture);
    let mut waiter = CountingWaiter::default();

    let result = fixture.run(&mut waiter);

    assert!(matches!(
        result,
        Err(LaunchHandoffError::LaunchStillInProgress { .. })
    ));
    assert_eq!(waiter.calls, 2);
}

struct Fixture {
    _state_dir: tempfile::TempDir,
    state_root: StateRoot,
    runtime_dir: tempfile::TempDir,
    endpoint_records: EndpointRecordStore,
    launch_lock_path: std::path::PathBuf,
    policy: LaunchHandoffPolicy,
}

impl Fixture {
    fn new() -> Self {
        Self::with_policy(LaunchHandoffPolicy {
            max_wait_attempts: 3,
        })
    }

    fn with_policy(policy: LaunchHandoffPolicy) -> Self {
        let state_dir = tempfile::TempDir::new().unwrap();
        let runtime_dir = tempfile::TempDir::new().unwrap();
        let state_root = StateRoot::resolve(state_dir.path()).unwrap();
        let endpoint_records = EndpointRecordStore::new(runtime_dir.path());
        let launch_lock_path = runtime_dir.path().join("launch.lock");
        Self {
            _state_dir: state_dir,
            state_root,
            runtime_dir,
            endpoint_records,
            launch_lock_path,
            policy,
        }
    }

    fn run<W: LaunchWaiter>(
        &self,
        waiter: &mut W,
    ) -> Result<LaunchHandoffResult, LaunchHandoffError> {
        self.run_requiring_protocol(APP_SERVER_PROTOCOL_VERSION, waiter)
    }

    fn run_requiring_protocol<W: LaunchWaiter>(
        &self,
        required_protocol_version: &str,
        waiter: &mut W,
    ) -> Result<LaunchHandoffResult, LaunchHandoffError> {
        let handoff = AttachOrLaunchHandoff::new(
            AttachOrLaunchRunner::new(self.endpoint_records.clone(), &self.launch_lock_path),
            self.policy,
        );
        handoff.run(
            self.state_root.fingerprint(),
            &AttachOrLaunchRequirements {
                required_protocol_version: required_protocol_version.to_string(),
                required_app_version: env!("CARGO_PKG_VERSION").to_string(),
            },
            StorageWriterState::Available,
            waiter,
        )
    }

    fn run_requiring_app_version<W: LaunchWaiter>(
        &self,
        required_app_version: &str,
        waiter: &mut W,
    ) -> Result<LaunchHandoffResult, LaunchHandoffError> {
        let handoff = AttachOrLaunchHandoff::new(
            AttachOrLaunchRunner::new(self.endpoint_records.clone(), &self.launch_lock_path),
            self.policy,
        );
        handoff.run(
            self.state_root.fingerprint(),
            &AttachOrLaunchRequirements {
                required_protocol_version: APP_SERVER_PROTOCOL_VERSION.to_string(),
                required_app_version: required_app_version.to_string(),
            },
            StorageWriterState::Available,
            waiter,
        )
    }
}

struct RemoveIncompatibleEndpointAfterWait {
    published: Option<crate::app_server_process::PublishedAppServerEndpoint>,
    calls: usize,
}

impl LaunchWaiter for RemoveIncompatibleEndpointAfterWait {
    fn wait_for_launch_progress(&mut self, _lock_path: &Path) -> Result<(), LaunchWaitError> {
        self.calls += 1;
        self.published.take();
        Ok(())
    }
}

#[derive(Default)]
struct CountingWaiter {
    calls: usize,
}

impl LaunchWaiter for CountingWaiter {
    fn wait_for_launch_progress(&mut self, _lock_path: &Path) -> Result<(), LaunchWaitError> {
        self.calls += 1;
        Ok(())
    }
}

struct NoopWaiter;

impl LaunchWaiter for NoopWaiter {
    fn wait_for_launch_progress(&mut self, _lock_path: &Path) -> Result<(), LaunchWaitError> {
        Ok(())
    }
}

struct PublishAfterWait {
    held_lock: Option<RuntimeLock>,
    state_root: StateRoot,
    runtime_root: std::path::PathBuf,
    published: Option<crate::app_server_process::PublishedAppServerEndpoint>,
    calls: usize,
}

impl LaunchWaiter for PublishAfterWait {
    fn wait_for_launch_progress(&mut self, _lock_path: &Path) -> Result<(), LaunchWaitError> {
        self.calls += 1;
        self.held_lock.take();
        let dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(self.state_root.clone());
        let published = crate::app_server_process::publish_local_http_probe_endpoint(
            dispatcher.shared_gateway(),
            &self.state_root,
            &self.runtime_root,
        )
        .map_err(|error| LaunchWaitError {
            message: error.to_string(),
        })?;
        self.published = Some(published);
        Ok(())
    }
}

fn acquire_launch_lock(fixture: &Fixture) -> RuntimeLock {
    match RuntimeLock::acquire(&fixture.launch_lock_path).unwrap() {
        LockAcquireOutcome::Acquired(lock) => lock,
        LockAcquireOutcome::Busy { .. } => panic!("test launch lock must be free"),
    }
}
