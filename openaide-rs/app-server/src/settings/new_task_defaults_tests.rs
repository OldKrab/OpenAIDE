use openaide_app_server_protocol::ids::{AgentId, ProjectId};
use openaide_app_server_protocol::settings::NewTaskDefaultsUpdateParams;
use openaide_app_server_protocol::snapshot::NewTaskDefaultsSnapshot;

use super::{NewTaskDefaultsService, NewTaskDefaultsWorkflow};
use crate::storage::Store;

#[test]
fn updating_project_default_preserves_the_other_new_task_default() {
    let root = tempfile::tempdir().expect("temporary state root");
    let store = Store::open(root.path().to_path_buf()).expect("open state root");
    store
        .write_new_task_defaults(&NewTaskDefaultsSnapshot {
            project_id: None,
            agent_id: Some(AgentId::from("codex")),
        })
        .expect("seed defaults");

    let result = NewTaskDefaultsService::new(store.clone())
        .update_defaults(NewTaskDefaultsUpdateParams {
            project_id: Some(ProjectId::from("project-api")),
            agent_id: None,
        })
        .expect("update defaults");

    assert_eq!(
        result,
        NewTaskDefaultsSnapshot {
            project_id: Some(ProjectId::from("project-api")),
            agent_id: Some(AgentId::from("codex")),
        }
    );
    assert_eq!(
        store.read_new_task_defaults().expect("read defaults"),
        result
    );
}

#[test]
fn updating_agent_default_preserves_the_project_default() {
    let root = tempfile::tempdir().expect("temporary state root");
    let store = Store::open(root.path().to_path_buf()).expect("open state root");
    store
        .write_new_task_defaults(&NewTaskDefaultsSnapshot {
            project_id: Some(ProjectId::from("project-api")),
            agent_id: Some(AgentId::from("codex")),
        })
        .expect("seed defaults");

    let result = NewTaskDefaultsService::new(store.clone())
        .update_defaults(NewTaskDefaultsUpdateParams {
            project_id: None,
            agent_id: Some(AgentId::from("cursor")),
        })
        .expect("update defaults");

    assert_eq!(
        result,
        NewTaskDefaultsSnapshot {
            project_id: Some(ProjectId::from("project-api")),
            agent_id: Some(AgentId::from("cursor")),
        }
    );
}
