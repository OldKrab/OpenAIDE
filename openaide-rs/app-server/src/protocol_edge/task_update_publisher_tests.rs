use std::sync::Mutex;
use std::time::Duration;

use openaide_app_server_protocol::events::{TaskChanges, TaskChatChange};
use openaide_app_server_protocol::ids::MessageId;
use serde_json::{json, Value};

use super::*;
use crate::task_events::CommittedTaskChange;

// timing: data — slow-batch thresholds compared against elapsed time and never waited on.
const NEVER_SLOW: Duration = Duration::MAX;
// timing: data — every batch is at least this slow, so the slow-batch path always logs.
const ALWAYS_SLOW: Duration = Duration::ZERO;

#[test]
fn queued_navigation_wakes_publish_once_since_one_flush_covers_every_mark() {
    let run = run_queued(
        vec![wake(), chat("task-1", "first"), wake(), wake()],
        NEVER_SLOW,
    );

    assert_eq!(run.published, vec![wake(), chat("task-1", "first")]);
}

#[test]
fn ordered_deltas_are_published_in_order_without_reducing_them_to_task_ids() {
    let updates = vec![
        chat("task-1", "raw chunk"),
        chat("task-1", "next chunk"),
        chat("task-2", "other task"),
    ];

    let run = run_queued(updates.clone(), NEVER_SLOW);

    assert_eq!(run.published, updates);
}

#[test]
fn coalesced_batch_reports_counts_and_protocol_lock_timing() {
    let run = run_queued(vec![wake(), chat("task-1", "chunk"), wake()], NEVER_SLOW);

    let batch = only_event(&run.logs, "task_update_batch_published");
    assert_eq!(batch["operation"], json!("task_update_publication"));
    assert_eq!(batch["outcome"], json!("published"));
    assert_eq!(batch["count"], json!(3));
    assert_eq!(batch["published_count"], json!(2));
    assert_eq!(batch["coalesced_count"], json!(1));
    // The fake gateway waits 5 ms per publication and holds the wake longest.
    assert_eq!(batch["wait_ms"], json!(10));
    assert_eq!(batch["max_hold_ms"], json!(40));
    assert_eq!(batch["update_kind"], json!("navigation_dirty"));
    let stopped = only_event(&run.logs, "task_update_publisher_stopped");
    assert_eq!(stopped["outcome"], json!("closed"));
    assert_eq!(stopped["count"], json!(3));
    assert_eq!(stopped["coalesced_count"], json!(1));
    assert_eq!(run.logs[0].0, "task_update_publisher_started");
}

#[test]
fn healthy_single_update_batch_stays_quiet() {
    let run = run_queued(vec![chat("task-1", "chunk")], NEVER_SLOW);

    assert!(events_named(&run.logs, "task_update_batch_published").is_empty());
}

#[test]
fn slow_batch_is_reported_without_coalescing() {
    let run = run_queued(vec![chat("task-1", "chunk")], ALWAYS_SLOW);

    let batch = only_event(&run.logs, "task_update_batch_published");
    assert_eq!(batch["count"], json!(1));
    assert_eq!(batch["coalesced_count"], json!(0));
    assert_eq!(batch["update_kind"], json!("changed"));
}

struct Run {
    published: Vec<TaskUpdate>,
    logs: Vec<(String, Value)>,
}

/// Queues every update before the publisher starts, so they form one batch, then runs
/// it until the dropped sender closes the channel.
fn run_queued(updates: Vec<TaskUpdate>, slow_batch: Duration) -> Run {
    let (sender, receiver) = std::sync::mpsc::channel();
    for update in updates {
        sender.send(update).unwrap();
    }
    drop(sender);
    let mut published = Vec::new();
    let logs = Mutex::new(Vec::new());

    publish_task_updates(
        receiver,
        slow_batch,
        |update| {
            published.push(update.clone());
            fake_lock_timing(&update.kind)
        },
        |event, fields| logs.lock().unwrap().push((event.to_string(), fields)),
    );

    Run {
        published,
        logs: logs.into_inner().unwrap(),
    }
}

fn fake_lock_timing(kind: &TaskUpdateKind) -> TaskUpdatePublication {
    // timing: data — reported lock timing values, never waited on.
    let held_ms = match kind {
        TaskUpdateKind::NavigationDirty => 40,
        _ => 2,
    };
    TaskUpdatePublication {
        lock_wait: Duration::from_millis(5),       // timing: data
        lock_held: Duration::from_millis(held_ms), // timing: data
    }
}

fn events_named<'a>(logs: &'a [(String, Value)], event: &str) -> Vec<&'a Value> {
    logs.iter()
        .filter(|(name, _)| name == event)
        .map(|(_, fields)| fields)
        .collect()
}

#[track_caller]
fn only_event<'a>(logs: &'a [(String, Value)], event: &str) -> &'a Value {
    match events_named(logs, event).as_slice() {
        [fields] => fields,
        other => panic!("expected one {event}, got {}", other.len()),
    }
}

fn wake() -> TaskUpdate {
    TaskUpdate {
        task_id: String::new(),
        revision: 0,
        kind: TaskUpdateKind::NavigationDirty,
    }
}

fn chat(task_id: &str, text: &str) -> TaskUpdate {
    TaskUpdate {
        task_id: task_id.to_string(),
        revision: 2,
        kind: TaskUpdateKind::Changed(Box::new(CommittedTaskChange {
            changes: TaskChanges {
                chat: vec![TaskChatChange::AppendText {
                    message_id: MessageId::from("message-1"),
                    text: text.to_string(),
                }],
                ..TaskChanges::default()
            },
            tool_details: Vec::new(),
            navigation: None,
        })),
    }
}
