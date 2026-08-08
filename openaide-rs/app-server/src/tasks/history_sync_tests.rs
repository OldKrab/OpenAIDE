use openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot;

use crate::snapshots::task_snapshot::TaskHistorySyncSnapshotSource;

use super::HistorySyncCoordinator;

#[test]
fn passive_checks_coalesce_until_the_current_generation_finishes() {
    let coordinator = HistorySyncCoordinator::default();
    let current = coordinator
        .begin_passive("task-history")
        .expect("history check generation");
    assert!(coordinator.begin_passive("task-history").is_none());
    let current_state = TaskHistorySyncSnapshot::Syncing {
        generation: current.value(),
    };

    assert!(coordinator.set_current("task-history", current_state.clone()));
    assert_eq!(
        coordinator.history_sync_snapshot("task-history"),
        current_state,
    );

    coordinator.finish_passive("task-history", &current);
    let next = coordinator
        .begin_passive("task-history")
        .expect("next history check generation");
    assert_ne!(next.value(), current.value());
}
