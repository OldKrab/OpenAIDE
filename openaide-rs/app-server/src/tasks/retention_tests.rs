use std::sync::Arc;

use crate::agent::registry::AgentRegistry;
use crate::client_lifecycle::AppServerTime;
use crate::projects::StorageProjectResolver;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{IsolationKind, TaskStatus};
use crate::storage::records::{
    QueuedMessageRecord, TaskLifecycle, TaskPreparationRecord, TaskRecord,
};
use crate::storage::Store;
use crate::task_events::TaskUpdateNotifier;
use crate::tasks::product_api::TaskProductApi;
use crate::{server_requests::ServerRequestDraft, state_sync::TaskSubscriptionPresence};
use openaide_app_server_protocol::ids::{ClientInstanceId, TaskId};
use openaide_app_server_protocol::snapshot::PendingRequestScope;

#[test]
fn retention_sweep_uses_three_day_idle_window() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    let store = Store::open(root.path().join("state")).unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let now = crate::time::activity_millis("2026-08-07T00:00:00Z").unwrap();
    let three_days_millis = 3 * 24 * 60 * 60 * 1_000;
    let cutoff = now - three_days_millis;

    for task_id in ["at-cutoff", "inside-window"] {
        let task = task_record(
            task_id,
            &workspace.to_string_lossy(),
            "2026-01-01T00:00:00Z".to_string(),
        );
        store.write_task(&task).unwrap();
    }
    api.mark_task_used_at_for_test("at-cutoff", cutoff).unwrap();
    api.mark_task_used_at_for_test("inside-window", cutoff + 1)
        .unwrap();

    let report = api.run_storage_maintenance_at(now).unwrap();

    assert_eq!(report.purged_tasks, 1);
    assert!(matches!(
        store.read_task("at-cutoff"),
        Err(RuntimeError::TaskNotFound(_))
    ));
    assert!(store.read_task("inside-window").is_ok());
}

#[test]
fn retention_sweep_purges_only_three_day_old_idle_unpinned_local_task_state() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    let store = Store::open(root.path().join("state")).unwrap();
    let server_requests = crate::server_requests::ServerRequestRuntime::new();
    let presence = TaskSubscriptionPresence::default();
    let api = TaskProductApi::new_with_server_requests(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
        server_requests.clone(),
    )
    .unwrap()
    .with_task_subscription_presence(presence.clone());

    let old = crate::time::activity_millis("2026-07-01T00:00:00Z").unwrap();
    let now = crate::time::activity_millis("2026-08-07T00:00:00Z").unwrap();
    for task_id in [
        "expired",
        "archived",
        "recently-opened",
        "pinned",
        "active",
        "queued",
        "subscribed",
        "pending-request",
        "prepared",
    ] {
        let mut task = task_record(task_id, &workspace.to_string_lossy(), old.to_string());
        match task_id {
            "archived" => task.lifecycle = TaskLifecycle::Archived,
            "pinned" => task.pinned = true,
            "active" => {
                task.status = TaskStatus::Active;
                task.active_turn_id = Some("turn-1".to_string());
            }
            "queued" => task.message_queue.items.push(QueuedMessageRecord {
                queued_message_id: "queued-1".to_string(),
                text: "later".to_string(),
                created_at: old.to_string(),
                chat_attachments: Vec::new(),
                agent_attachments: Vec::new(),
            }),
            "prepared" => task.lifecycle = TaskLifecycle::Prepared { lease: None },
            _ => {}
        }
        store.write_task(&task).unwrap();
        api.mark_task_used_at_for_test(task_id, old).unwrap();
    }
    api.mark_task_used_at_for_test("recently-opened", now)
        .unwrap();
    presence.subscribe_for_test(
        ClientInstanceId::from("client-1"),
        TaskId::from("subscribed"),
    );
    server_requests.open(
        ServerRequestDraft {
            scope: PendingRequestScope::Task {
                task_id: TaskId::from("pending-request"),
            },
            method: "permission/request".to_string(),
            title: "Permission".to_string(),
            params: serde_json::json!({}),
        },
        Vec::new(),
        AppServerTime(now as u64),
    );

    let report = api.run_storage_maintenance_at(now).unwrap();

    assert_eq!(report.purged_tasks, 2);
    for purged in ["expired", "archived"] {
        assert!(matches!(
            store.read_task(purged),
            Err(RuntimeError::TaskNotFound(_))
        ));
    }
    for retained in [
        "recently-opened",
        "pinned",
        "active",
        "queued",
        "subscribed",
        "pending-request",
        "prepared",
    ] {
        assert!(store.read_task(retained).is_ok(), "{retained} was purged");
    }
}

#[test]
fn opening_a_task_updates_only_its_small_retention_marker() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    let store = Store::open(root.path().join("state")).unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let mut task = task_record(
        "opened",
        &workspace.to_string_lossy(),
        "2026-01-01T00:00:00Z".to_string(),
    );
    task.agent_session_id = None;
    store.write_task(&task).unwrap();
    let revision = store.read_task("opened").unwrap().revision;

    api.open_for_test(openaide_app_server_protocol::task::TaskOpenParams {
        task_id: TaskId::from("opened"),
    })
    .unwrap();

    assert_eq!(store.read_task("opened").unwrap().revision, revision);
    assert!(store
        .task_journal()
        .task_last_used_millis("opened")
        .unwrap()
        .is_some());
}

#[test]
fn a_legacy_task_without_a_usage_marker_gets_one_tracked_grace_period() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    let store = Store::open(root.path().join("state")).unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let old = crate::time::activity_millis("2026-01-01T00:00:00Z").unwrap();
    let first_sweep = crate::time::activity_millis("2026-08-07T00:00:00Z").unwrap();
    store
        .write_task(&task_record(
            "legacy-untracked",
            &workspace.to_string_lossy(),
            old.to_string(),
        ))
        .unwrap();

    let first_report = api.run_storage_maintenance_at(first_sweep).unwrap();

    assert_eq!(first_report.purged_tasks, 0);
    assert_eq!(
        store
            .task_journal()
            .task_last_used_millis("legacy-untracked")
            .unwrap(),
        Some(first_sweep)
    );

    let after_grace = first_sweep + crate::tasks::retention::DEFAULT_TASK_RETENTION_MILLIS + 1;
    let second_report = api.run_storage_maintenance_at(after_grace).unwrap();

    assert_eq!(second_report.purged_tasks, 1);
    assert!(matches!(
        store.read_task("legacy-untracked"),
        Err(RuntimeError::TaskNotFound(_))
    ));
}

#[test]
fn a_failed_usage_marker_replacement_preserves_the_previous_complete_value() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    let state_root = root.path().join("state");
    let store = Store::open(state_root.clone()).unwrap();
    let api = TaskProductApi::new(
        store.clone(),
        Arc::new(StorageProjectResolver::new(store.clone())),
        AgentRegistry::default_built_ins(),
        Arc::new(crate::agent::mock::MockAgent),
        TaskUpdateNotifier::disabled(),
    )
    .unwrap();
    let old = 1_000;
    store
        .write_task(&task_record(
            "atomic-marker",
            &workspace.to_string_lossy(),
            old.to_string(),
        ))
        .unwrap();
    api.mark_task_used_at_for_test("atomic-marker", old)
        .unwrap();
    std::fs::create_dir(state_root.join("task-store-v1/tasks/atomic-marker/last-used.tmp"))
        .unwrap();

    assert!(api
        .mark_task_used_at_for_test("atomic-marker", old + 1)
        .is_err());
    assert_eq!(
        store
            .task_journal()
            .task_last_used_millis("atomic-marker")
            .unwrap(),
        Some(old)
    );
}

fn task_record(task_id: &str, workspace: &str, activity: String) -> TaskRecord {
    TaskRecord {
        task_id: task_id.to_string(),
        title: Default::default(),
        status: TaskStatus::Inactive,
        task_version: 1,
        message_history_version: 0,
        unread: false,
        pinned: false,
        attention: None,
        created_at: activity.clone(),
        updated_at: activity.clone(),
        last_activity: activity,
        composer_history: Default::default(),
        message_queue: Default::default(),
        agent_id: "codex".to_string(),
        agent_name: "Codex".to_string(),
        isolation: IsolationKind::Local,
        workspace_root: workspace.to_string(),
        project_root: None,
        worktree_id: None,
        lifecycle: TaskLifecycle::Open,
        agent_session_id: Some("native-session-preserved".to_string()),
        active_turn_id: None,
        active_turn_started_at: None,
        tombstoned: false,
        revision: 1,
        config_options_catalog: None,
        native_session_data_freshness: Default::default(),
        config_mutation: Default::default(),
        agent_commands_catalog: None,
        context_usage: None,
        current_plan: None,
        completed_plan_message_id: None,
        last_turn_usage: None,
        model_id: None,
        supports_image_input: false,
        preparation: TaskPreparationRecord::Ready,
    }
}
