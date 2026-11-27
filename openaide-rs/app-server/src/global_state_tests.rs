use serde_json::{json, Value};

use super::*;

fn project(id: &str, repository: Option<&str>) -> Value {
    let mut project = json!({
        "projectId": id,
        "label": id,
        "workspaceRoot": format!("/work/{id}"),
        "available": true,
    });
    if let Some(repository) = repository {
        project["worktreeRepositoryId"] = json!(repository);
    }
    project
}

fn repository(id: &str, revision: u64) -> WorktreeRepositorySnapshot {
    serde_json::from_value(json!({
        "repositoryId": id,
        "revision": revision,
        "worktrees": [],
    }))
    .unwrap()
}

fn state(projects: Vec<Value>, repositories: Vec<WorktreeRepositorySnapshot>) -> GlobalState {
    let mut state = empty_state_for_tests();
    state.projects = serde_json::from_value(json!({ "projects": projects })).unwrap();
    state.worktrees = repositories;
    state
}

#[test]
fn identical_states_have_no_changes() {
    let a = state(vec![project("p1", Some("r1"))], vec![repository("r1", 1)]);
    assert!(diff(&a, &a.clone()).is_empty());
}

#[test]
fn a_worktree_revision_bump_alone_is_not_a_change() {
    let old = state(vec![project("p1", Some("r1"))], vec![repository("r1", 1)]);
    let new = state(vec![project("p1", Some("r1"))], vec![repository("r1", 2)]);
    assert!(diff(&old, &new).is_empty());
}

#[test]
fn one_diff_reports_changes_across_areas() {
    let old = state(vec![project("p1", Some("r1"))], vec![repository("r1", 1)]);
    let mut new = state(
        vec![project("p1", Some("r1")), project("p2", Some("r2"))],
        vec![repository("r2", 1)],
    );
    new.new_task_defaults.project_id = Some("p2".into());
    let changes = diff(&old, &new);
    assert_eq!(changes.projects.as_ref().unwrap().projects.len(), 2);
    assert_eq!(changes.worktrees.len(), 1);
    assert_eq!(changes.removed_worktree_repositories.len(), 1);
    assert!(changes.new_task_defaults.is_some());
    assert!(changes.agents.is_none() && changes.settings.is_none());
}

#[test]
fn filter_keeps_only_the_listed_projects_and_their_repositories() {
    let full = state(
        vec![project("p1", Some("r1")), project("p2", Some("r2"))],
        vec![repository("r1", 1), repository("r2", 1)],
    );
    let filtered = filter_state(&full, &["p2".into()]);
    assert_eq!(filtered.projects.projects.len(), 1);
    assert_eq!(filtered.worktrees.len(), 1);
    assert_eq!(filtered.worktrees[0].repository_id, "r2".into());
}

#[test]
fn a_repository_shared_by_two_projects_stays_while_one_remains() {
    let full = state(
        vec![project("p1", Some("r1")), project("p2", Some("r1"))],
        vec![repository("r1", 1)],
    );
    let filtered = filter_state(&full, &["p1".into()]);
    assert_eq!(filtered.worktrees.len(), 1);
}

#[test]
fn upsert_ignores_a_repeat_and_replaces_real_changes() {
    let mut repositories = vec![repository("r1", 1)];
    upsert_repository(&mut repositories, repository("r1", 2));
    assert_eq!(repositories[0].revision, 1);
    let mut changed = repository("r1", 3);
    changed.operations = Vec::new();
    changed.worktrees = serde_json::from_value(json!([{
        "worktreeId": "w1", "path": "/w", "branch": "main", "kind": "main",
    }]))
    .unwrap_or_default();
    upsert_repository(&mut repositories, changed);
    assert!(repositories.len() == 1);
}

mod owner {
    use std::sync::{Arc, Mutex};

    use openaide_app_server_protocol::snapshot::{ProjectCollectionSnapshot, ProjectSummary};

    use super::*;
    use crate::agent::registry::AgentRegistry;
    use crate::settings::SettingsCatalog;
    use crate::snapshots::{
        AgentRegistrySnapshotSource, ProjectCollectionSnapshotSource, SnapshotSources,
        TaskSnapshotStore, WorktreeRepositorySnapshotSource,
    };
    use crate::storage::Store;

    /// Projects the test edits, each naming the repository it uses.
    #[derive(Clone, Default)]
    struct Projects(Arc<Mutex<Vec<(String, String)>>>);

    impl Projects {
        fn add(&self, id: &str, repository: &str) {
            self.0.lock().unwrap().push((id.into(), repository.into()));
        }
    }

    impl ProjectCollectionSnapshotSource for Projects {
        fn snapshot(&self) -> Result<ProjectCollectionSnapshot, ProtocolError> {
            Ok(ProjectCollectionSnapshot {
                projects: self
                    .0
                    .lock()
                    .unwrap()
                    .iter()
                    .map(|(id, repository)| ProjectSummary {
                        project_id: id.as_str().into(),
                        label: id.clone(),
                        workspace_root: format!("/work/{id}"),
                        available: true,
                        worktree_repository_id: Some(repository.as_str().into()),
                        project_worktree_id: None,
                        worktree_error: None,
                    })
                    .collect(),
            })
        }
    }

    /// Every read is a refresh: the revision counts reads, the content never changes.
    #[derive(Default)]
    struct Repositories(Mutex<u64>);

    impl WorktreeRepositorySnapshotSource for Repositories {
        fn snapshot(
            &self,
            repository_id: &WorktreeRepositoryId,
        ) -> Result<WorktreeRepositorySnapshot, ProtocolError> {
            let mut reads = self.0.lock().unwrap();
            *reads += 1;
            Ok(repository(repository_id.as_str(), *reads))
        }
    }

    fn sources(projects: Projects) -> (SnapshotBuilder, tempfile::TempDir) {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path().to_path_buf()).unwrap();
        let sources = SnapshotSources::new(
            Arc::new(store.clone()),
            Arc::new(AgentRegistrySnapshotSource::new(
                AgentRegistry::default_built_ins(),
            )),
            Arc::new(projects),
            Arc::new(Repositories::default()),
            Arc::new(SettingsCatalog::default()),
            Arc::new(TaskSnapshotStore::new(store)),
        );
        (
            SnapshotBuilder::with_sources("server-1".into(), "root-1".into(), sources),
            root,
        )
    }

    fn everyone() -> GlobalStateAudience {
        GlobalStateAudience {
            client_instance_id: "web".into(),
            project_filter: None,
        }
    }

    fn vscode(projects: &[&str]) -> GlobalStateAudience {
        GlobalStateAudience {
            client_instance_id: "vscode".into(),
            project_filter: Some(projects.iter().map(|id| (*id).into()).collect()),
        }
    }

    #[test]
    fn the_initial_state_carries_every_area() {
        let projects = Projects::default();
        projects.add("p1", "r1");
        let (sources, _root) = sources(projects);
        let mut owner = GlobalStateOwner::default();

        let state = owner.state_for(&sources, &everyone()).unwrap();

        assert_eq!(state.projects.projects.len(), 1);
        assert_eq!(state.worktrees.len(), 1);
        assert!(!state.agents.agents.is_empty());
        assert!(!state.settings.sections.is_empty());
    }

    #[test]
    fn a_refresh_that_finds_nothing_new_publishes_nothing() {
        let projects = Projects::default();
        projects.add("p1", "r1");
        let (sources, _root) = sources(projects);
        let mut owner = GlobalStateOwner::default();
        owner.state_for(&sources, &everyone()).unwrap();
        let everyone = [everyone()];

        // Reading the repository again bumps its revision but changes nothing a client shows.
        let all = owner.refresh(&sources, GlobalAreas::WORKTREES, None, &everyone);
        let projects_only = owner.refresh(&sources, GlobalAreas::PROJECTS, None, &everyone);
        let agents = owner.refresh(&sources, GlobalAreas::AGENTS, None, &everyone);

        assert!(all.is_empty() && projects_only.is_empty() && agents.is_empty());
    }

    #[test]
    fn a_project_added_later_brings_its_worktrees_in_the_same_change() {
        let projects = Projects::default();
        projects.add("p1", "r1");
        let (sources, _root) = sources(projects.clone());
        let mut owner = GlobalStateOwner::default();
        owner.state_for(&sources, &everyone()).unwrap();

        projects.add("p2", "r2");
        let sent = owner.refresh(&sources, GlobalAreas::PROJECTS, None, &[everyone()]);

        let [(client, changes)] = sent.as_slice() else {
            panic!("expected one delivery, got {sent:?}");
        };
        assert_eq!(client.as_str(), "web");
        assert_eq!(changes.projects.as_ref().unwrap().projects.len(), 2);
        let ids: Vec<_> = changes
            .worktrees
            .iter()
            .map(|repository| repository.repository_id.as_str())
            .collect();
        assert_eq!(ids, ["r2"], "only the new repository is sent");
    }

    #[test]
    fn vscode_gets_only_its_projects_and_their_repositories() {
        let projects = Projects::default();
        projects.add("p1", "r1");
        let (sources, _root) = sources(projects.clone());
        let mut owner = GlobalStateOwner::default();
        let initial = owner.state_for(&sources, &vscode(&["p1"])).unwrap();
        assert_eq!(initial.projects.projects.len(), 1);

        // A project VS Code does not use: nothing reaches it, everyone else gets the change.
        projects.add("p2", "r2");
        let audiences = [everyone(), vscode(&["p1"])];
        let sent = owner.refresh(&sources, GlobalAreas::PROJECTS, None, &audiences);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0.as_str(), "web");

        // Once VS Code's workspace includes it, VS Code gets that project and its repository.
        let audiences = [everyone(), vscode(&["p1", "p2"])];
        let sent = owner.refresh(&sources, GlobalAreas::PROJECTS, None, &audiences);
        let (client, changes) = sent
            .iter()
            .find(|(client, _)| client.as_str() == "vscode")
            .expect("vscode receives its new project");
        assert_eq!(client.as_str(), "vscode");
        assert_eq!(changes.projects.as_ref().unwrap().projects.len(), 2);
        assert_eq!(changes.worktrees.len(), 1);
        assert_eq!(changes.worktrees[0].repository_id.as_str(), "r2");
    }

    #[test]
    fn a_fresh_owner_gives_a_reconnecting_client_the_full_state() {
        let projects = Projects::default();
        projects.add("p1", "r1");
        let (sources, _root) = sources(projects.clone());
        let mut owner = GlobalStateOwner::default();
        owner.state_for(&sources, &everyone()).unwrap();
        projects.add("p2", "r2");
        owner.refresh(&sources, GlobalAreas::PROJECTS, None, &[everyone()]);

        // A client that lost its session initializes again and is not sent a delta.
        let again = owner.state_for(&sources, &everyone()).unwrap();

        assert_eq!(again.projects.projects.len(), 2);
        assert_eq!(again.worktrees.len(), 2);
    }
}
