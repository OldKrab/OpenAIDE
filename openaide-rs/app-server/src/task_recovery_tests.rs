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
        })
    );
}

fn task_record() -> TaskRecord {
    TaskRecord {
        task_id: "task-1".to_string(),
        title: "Task".to_string(),
        agent_title: None,
        status: TaskStatus::Inactive,
        task_version: 1,
        message_history_version: 0,
        unread: false,
        created_at: "2026-01-01T00:00:00.000Z".to_string(),
        updated_at: "2026-01-01T00:00:00.000Z".to_string(),
        last_activity: "2026-01-01T00:00:00.000Z".to_string(),
        agent_id: "codex".to_string(),
        agent_name: "Codex".to_string(),
        isolation: IsolationKind::Local,
        workspace_root: "/workspace".to_string(),
        first_prompt_sent: false,
        agent_session_id: None,
        active_turn_id: None,
        archived: false,
        tombstoned: false,
        revision: 0,
        config_options: Default::default(),
        config_options_catalog: None,
        agent_commands_catalog: None,
        model_id: None,
        preparation: TaskPreparationRecord::Ready,
    }
}
