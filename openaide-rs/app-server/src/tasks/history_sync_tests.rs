use std::sync::mpsc;
use std::time::Duration;

use openaide_app_server_protocol::snapshot::TaskHistorySyncSnapshot;

use crate::snapshots::task_snapshot::TaskHistorySyncSnapshotSource;

use super::HistorySyncCoordinator;

#[test]
fn superseding_a_send_wakes_its_state_wait_and_releases_the_task_operation() {
    let coordinator = HistorySyncCoordinator::default();
    let waiting_generation = coordinator.begin_send("task-waiting");
    let waiting_coordinator = coordinator.clone();
    let (waiting_tx, waiting_rx) = mpsc::channel();

    let waiting = std::thread::spawn(move || {
        waiting_coordinator.run_send("task-waiting", waiting_generation, || {
            waiting_coordinator.wait_for_current_send("task-waiting", waiting_generation, || {
                waiting_tx.send(()).unwrap();
                Ok::<Option<()>, ()>(None)
            })
        })
    });
    waiting_rx
        .recv_timeout(Duration::from_millis(250))
        .expect("the first send should be waiting for Task state");

    let current_generation = coordinator.begin_send("task-waiting");
    let superseded = waiting
        .join()
        .expect("the superseded send worker should stop");
    let current = coordinator.run_send("task-waiting", current_generation, || "current");

    assert_eq!(superseded, Some(Ok(None)));
    assert_eq!(current, Some("current"));
}

#[test]
fn superseded_generation_cannot_replace_the_current_history_snapshot() {
    let coordinator = HistorySyncCoordinator::default();
    let passive = coordinator
        .begin_passive("task-history")
        .expect("an idle Task can start passive reconciliation");
    assert!(coordinator.set_current(
        "task-history",
        TaskHistorySyncSnapshot::Checking {
            generation: passive.value(),
        },
    ));

    let send_generation = coordinator.begin_send("task-history");
    let current = TaskHistorySyncSnapshot::Syncing {
        generation: send_generation,
    };
    assert!(coordinator.set_current("task-history", current.clone()));

    assert!(!coordinator.set_current(
        "task-history",
        TaskHistorySyncSnapshot::Failed {
            generation: passive.value(),
            message: "superseded refresh failed".to_string(),
            before_send: false,
        },
    ));
    assert_eq!(coordinator.history_sync_snapshot("task-history"), current);
}

#[test]
fn passive_reconciliation_cannot_supersede_an_active_send_generation() {
    let coordinator = HistorySyncCoordinator::default();
    let send_generation = coordinator.begin_send("task-history");
    let syncing = TaskHistorySyncSnapshot::Syncing {
        generation: send_generation,
    };
    assert!(coordinator.set_current("task-history", syncing.clone()));

    assert!(coordinator.begin_passive("task-history").is_none());
    assert_eq!(coordinator.history_sync_snapshot("task-history"), syncing);

    assert!(coordinator.set_current(
        "task-history",
        TaskHistorySyncSnapshot::Failed {
            generation: send_generation,
            message: "History must be synchronized before sending".to_string(),
            before_send: true,
        },
    ));
    assert!(coordinator.begin_passive("task-history").is_none());

    assert!(coordinator.set_current(
        "task-history",
        TaskHistorySyncSnapshot::Updated {
            generation: send_generation,
        },
    ));
    assert!(coordinator.begin_passive("task-history").is_some());
}
