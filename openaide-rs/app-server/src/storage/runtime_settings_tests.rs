use super::Store;

#[test]
fn acp_trace_setting_defaults_off_and_round_trips() {
    let root = tempfile::tempdir().expect("create state root");
    let store = Store::open(root.path().to_path_buf()).expect("open store");

    assert!(!store.read_acp_trace_enabled().expect("read default"));
    store
        .write_acp_trace_enabled(true)
        .expect("write enabled setting");
    assert!(store
        .read_acp_trace_enabled()
        .expect("read enabled setting"));
}
