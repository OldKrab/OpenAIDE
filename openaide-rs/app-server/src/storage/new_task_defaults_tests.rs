use openaide_app_server_protocol::ids::{AgentId, ProjectId};
use openaide_app_server_protocol::snapshot::NewTaskDefaultsSnapshot;

use crate::storage::Store;

#[test]
fn new_task_defaults_round_trip_through_state_root_storage() {
    let root = tempfile::tempdir().expect("create state root");
    let store = Store::open(root.path().to_path_buf()).expect("open store");
    let defaults = NewTaskDefaultsSnapshot {
        project_id: Some(ProjectId::from("project-two")),
        agent_id: Some(AgentId::from("opencode")),
    };

    store
        .write_new_task_defaults(&defaults)
        .expect("persist defaults");
    drop(store);

    let reopened = Store::open(root.path().to_path_buf()).expect("reopen store");
    assert_eq!(
        reopened.read_new_task_defaults().expect("read defaults"),
        defaults
    );
}
