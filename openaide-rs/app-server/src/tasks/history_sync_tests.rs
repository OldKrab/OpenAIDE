use openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot;

use crate::snapshots::task_snapshot::TaskHistorySyncSnapshotSource;

use super::HistorySyncCoordinator;

#[test]
fn stale_passive_generation_cannot_replace_current_history_state() {
    let coordinator = HistorySyncCoordinator::default();
    let stale = coordinator
        .begin_passive("task-history")
        .expect("history check generation");
    let current = coordinator
        .begin_passive("task-history")
        .expect("newer history check generation");
    let current_state = TaskHistorySyncSnapshot::Syncing {
        generation: current.value(),
    };

    assert!(coordinator.set_current("task-history", current_state.clone()));
    assert!(!coordinator.set_current(
        "task-history",
        TaskHistorySyncSnapshot::Failed {
            generation: stale.value(),
            message: "stale refresh failed".to_string(),
            before_send: false,
        },
    ));
    assert_eq!(
        coordinator.history_sync_snapshot("task-history"),
        current_state,
    );
}
