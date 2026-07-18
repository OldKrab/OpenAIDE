use super::{volatile_recovery_plan, VolatileRecoveryPlan};
use crate::protocol::model::{IsolationKind, TaskStatus};
use crate::storage::records::{TaskPreparationRecord, TaskRecord};

#[test]
fn volatile_recovery_interrupts_active_turns_without_clearing_native_session_identity() {
    let mut task = task_record();
    assert_eq!(volatile_recovery_plan(&task), None);

    task.agent_session_id = Some("session-1".to_string());
    assert_eq!(volatile_recovery_plan(&task), None);

    task.active_turn_id = Some("turn-1".to_string());
    assert_eq!(
        volatile_recovery_plan(&task),
        Some(VolatileRecoveryPlan {
            interrupt_active_turn: true,
            invalidate_live_session_data: false,
            clear_pending_config_change: false,
        })
    );
}

#[test]
fn volatile_recovery_invalidates_native_session_catalogs() {
    let mut task = task_record();
    task.config_options_catalog = Some(crate::protocol::model::ConfigOptionsCatalog {
        agent_id: "codex".to_string(),
        status: crate::protocol::model::ConfigOptionsStatus::Ready,
        options: Vec::new(),
    });

    assert_eq!(
        volatile_recovery_plan(&task),
        Some(VolatileRecoveryPlan {
            interrupt_active_turn: false,
            invalidate_live_session_data: true,
            clear_pending_config_change: false,
        })
    );
}

#[test]
fn volatile_recovery_retires_an_interrupted_config_mutation() {
    let mut task = task_record();
    crate::tasks::config_options::begin_task_config_mutation(
        &mut task,
        "mutation-1".to_string(),
        "model".to_string(),
        "gpt-5.5".to_string(),
    )
    .unwrap();

    assert_eq!(
        volatile_recovery_plan(&task),
        Some(VolatileRecoveryPlan {
            interrupt_active_turn: false,
            invalidate_live_session_data: false,
            clear_pending_config_change: true,
        })
    );
}

fn task_record() -> TaskRecord {
    TaskRecord {
        task_id: "task-1".to_string(),
        title: crate::storage::records::TaskTitle::new(
            "Task",
            crate::storage::records::TaskTitleSource::User,
        ),
        status: TaskStatus::Inactive,
        task_version: 1,
        message_history_version: 0,
        unread: false,
        attention: None,
        created_at: "2026-01-01T00:00:00.000Z".to_string(),
        updated_at: "2026-01-01T00:00:00.000Z".to_string(),
        last_activity: "2026-01-01T00:00:00.000Z".to_string(),
        agent_id: "codex".to_string(),
        agent_name: "Codex".to_string(),
        isolation: IsolationKind::Local,
        workspace_root: "/workspace".to_string(),
        project_root: None,
        worktree_id: None,
        lifecycle: crate::storage::records::TaskLifecycle::New {
            lease: Some(openaide_app_server_protocol::ids::ClientInstanceId::from(
                "test-client",
            )),
        },
        agent_session_id: None,
        active_turn_id: None,
        active_turn_started_at: None,
        archived: false,
        tombstoned: false,
        revision: 0,
        config_options: Default::default(),
        config_options_catalog: None,
        config_mutation: Default::default(),
        agent_commands_catalog: None,
        model_id: None,
        supports_image_input: false,
        preparation: TaskPreparationRecord::Ready,
    }
}
