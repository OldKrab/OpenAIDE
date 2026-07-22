use super::*;

#[test]
fn same_state_root_yields_same_fingerprint() {
    let dir = tempfile::tempdir().unwrap();

    let first = StateRoot::resolve(dir.path()).unwrap();
    let second = StateRoot::resolve(dir.path()).unwrap();

    assert_eq!(first.fingerprint(), second.fingerprint());
}

#[test]
fn different_state_roots_do_not_collide_in_normal_cases() {
    let first_dir = tempfile::tempdir().unwrap();
    let second_dir = tempfile::tempdir().unwrap();

    let first = StateRoot::resolve(first_dir.path()).unwrap();
    let second = StateRoot::resolve(second_dir.path()).unwrap();

    assert_ne!(first.fingerprint(), second.fingerprint());
}

#[test]
fn endpoint_records_live_under_runtime_root_not_state_root() {
    let state_root = tempfile::tempdir().unwrap();
    let runtime_root = tempfile::tempdir().unwrap();
    let state_root = StateRoot::resolve(state_root.path()).unwrap();
    let store = EndpointRecordStore::new(runtime_root.path());
    let record = endpoint_record(state_root.fingerprint());

    let write = store.write(state_root.fingerprint(), &record).unwrap();

    assert!(store.is_runtime_path_under(&write.path));
    assert!(!write.path.starts_with(state_root.path()));
    assert_eq!(store.read(state_root.fingerprint()).unwrap(), Some(record));
}

#[test]
fn endpoint_record_remove_reports_stale_cleanup() {
    let state_root = tempfile::tempdir().unwrap();
    let runtime_root = tempfile::tempdir().unwrap();
    let state_root = StateRoot::resolve(state_root.path()).unwrap();
    let store = EndpointRecordStore::new(runtime_root.path());
    store
        .write(
            state_root.fingerprint(),
            &endpoint_record(state_root.fingerprint()),
        )
        .unwrap();

    assert!(store.remove(state_root.fingerprint()).unwrap());
    assert!(!store.remove(state_root.fingerprint()).unwrap());
}

#[test]
fn endpoint_record_conditional_remove_preserves_non_matching_record() {
    let state_root = tempfile::tempdir().unwrap();
    let runtime_root = tempfile::tempdir().unwrap();
    let state_root = StateRoot::resolve(state_root.path()).unwrap();
    let store = EndpointRecordStore::new(runtime_root.path());
    let record = endpoint_record(state_root.fingerprint());
    store.write(state_root.fingerprint(), &record).unwrap();

    assert!(!store
        .remove_if(state_root.fingerprint(), |current| current.server_id
            == "other")
        .unwrap());
    assert_eq!(store.read(state_root.fingerprint()).unwrap(), Some(record));
}

#[test]
fn endpoint_record_conditional_remove_removes_matching_record() {
    let state_root = tempfile::tempdir().unwrap();
    let runtime_root = tempfile::tempdir().unwrap();
    let state_root = StateRoot::resolve(state_root.path()).unwrap();
    let store = EndpointRecordStore::new(runtime_root.path());
    store
        .write(
            state_root.fingerprint(),
            &endpoint_record(state_root.fingerprint()),
        )
        .unwrap();

    assert!(store
        .remove_if(state_root.fingerprint(), |current| current.server_id
            == "server-1")
        .unwrap());
    assert_eq!(store.read(state_root.fingerprint()).unwrap(), None);
}

#[test]
fn endpoint_record_write_rejects_fingerprint_mismatch() {
    let state_root = tempfile::tempdir().unwrap();
    let runtime_root = tempfile::tempdir().unwrap();
    let state_root = StateRoot::resolve(state_root.path()).unwrap();
    let store = EndpointRecordStore::new(runtime_root.path());
    let mut record = endpoint_record(state_root.fingerprint());
    record.state_root_fingerprint = "other-root".to_string();

    assert!(matches!(
        store.write(state_root.fingerprint(), &record),
        Err(EndpointRecordStoreError::FingerprintMismatch)
    ));
}

#[test]
fn launch_lock_elects_single_owner() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("launch.lock");

    let first = RuntimeLock::acquire(&path).unwrap();
    let second = RuntimeLock::acquire(&path).unwrap();

    assert!(matches!(first, LockAcquireOutcome::Acquired(_)));
    assert!(matches!(second, LockAcquireOutcome::Busy { .. }));
}

#[test]
fn runtime_lock_releases_on_drop() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("writer.lock");

    let first = RuntimeLock::acquire(&path).unwrap();
    drop(first);
    let second = RuntimeLock::acquire(&path).unwrap();

    assert!(matches!(second, LockAcquireOutcome::Acquired(_)));
}

#[test]
fn existing_lock_file_does_not_block_without_live_owner() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("writer.lock");
    std::fs::write(&path, b"stale").unwrap();

    let acquired = RuntimeLock::acquire(&path).unwrap();

    assert!(matches!(acquired, LockAcquireOutcome::Acquired(_)));
}

fn endpoint_record(fingerprint: &StateRootFingerprint) -> RuntimeEndpointRecord {
    RuntimeEndpointRecord {
        server_id: "server-1".to_string(),
        state_root_fingerprint: fingerprint.as_str().to_string(),
        pid: std::process::id(),
        protocol_version: "1".to_string(),
        app_version: "0.1.0".to_string(),
        status: RuntimeEndpointRecordStatus::Running,
        auth_token: "token".to_string(),
        replacement_token: Some("replacement-token".to_string()),
        endpoints: vec![RuntimeEndpoint {
            transport: TransportKind::LocalHttp,
            address: "http://127.0.0.1:12345".to_string(),
        }],
    }
}
