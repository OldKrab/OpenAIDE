use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};

use tempfile::tempdir;

use super::super::SubagentCapabilitiesRecord;
use super::sweep;
use crate::protocol::model::{AgentMessagePart, NormalizedMessage};
use crate::storage::Store;

/// A Subagent whose journal the schema-1 writer left: one full-history frame per
/// change, each repeating every earlier message. Returns its history directory.
fn legacy_subagent(root: &Path, frames: u64, text_bytes: usize) -> (Store, String, PathBuf) {
    let store = Store::open(root.to_path_buf()).unwrap();
    let record = store
        .record_subagent_spawn(
            "task_1",
            "root-session",
            "child-session",
            "Researcher".to_string(),
            "Inspect".to_string(),
            SubagentCapabilitiesRecord::default(),
            Vec::new(),
        )
        .unwrap();
    let history_dir = root
        .join("task-store-v1/tasks/task_1/subagents")
        .join(&record.subagent_id);
    let mut journal = Vec::new();
    let mut messages = Vec::new();
    for revision in 1..=frames {
        messages.push(serde_json::json!({
            "sequence": revision,
            "chat": {
                "cursor": format!("m:{revision}"),
                "identity": format!("message-{revision}"),
                "message_type": "agent_message",
                "message_id": format!("message-{revision}"),
                "message": {
                    "kind": "agent_message",
                    "id": format!("message-{revision}"),
                    "role": "agent",
                    "parts": [{ "kind": "text", "text": format!("{revision}:{}", "x".repeat(text_bytes)) }],
                    "created_at": "1"
                }
            }
        }));
        let frame = serde_json::json!({
            "revision": revision,
            "history": {
                "schemaVersion": 1,
                "revision": revision,
                "journalFrames": revision,
                "messages": messages,
                "messageMeta": {
                    "task_id": "task_1",
                    "version": revision,
                    "message_count": revision,
                    "local_history_updated_at": "1",
                    "first_cursor": "m:1",
                    "last_cursor": format!("m:{revision}")
                },
                "currentPlan": null
            }
        });
        journal.extend(serde_json::to_vec(&frame).unwrap());
        journal.push(b'\n');
    }
    std::fs::write(history_dir.join("history.journal"), journal).unwrap();
    (store, record.subagent_id, history_dir)
}

fn journal_len(history_dir: &Path) -> u64 {
    std::fs::metadata(history_dir.join("history.journal"))
        .unwrap()
        .len()
}

fn assert_last_frame_history(store: &Store, subagent_id: &str, frames: u64) {
    let history = store.subagent_history("task_1", subagent_id).unwrap();
    assert_eq!(history.revision, frames);
    assert_eq!(history.messages.len(), frames as usize);
    assert!(matches!(
        &history.messages.last().unwrap().chat.message,
        NormalizedMessage::AgentMessage { parts, .. }
            if matches!(&parts[..], [AgentMessagePart::Text { text }] if text.starts_with(&format!("{frames}:")))
    ));
}

#[test]
fn a_journal_of_full_history_frames_becomes_the_snapshot_when_loaded() {
    let temp = tempdir().unwrap();
    let (store, subagent_id, history_dir) = legacy_subagent(temp.path(), 3, 16);

    assert_last_frame_history(&store, &subagent_id, 3);

    assert_eq!(
        journal_len(&history_dir),
        0,
        "the next load reads the snapshot alone"
    );
    drop(store);
    let reopened = Store::open(temp.path().to_path_buf()).unwrap();
    assert_last_frame_history(&reopened, &subagent_id, 3);
    reopened
        .append_subagent_message(
            "task_1",
            "child-session",
            NormalizedMessage::User {
                id: "user-message".to_string(),
                text: "Check the result".to_string(),
                created_at: "2".to_string(),
                attachments: Vec::new(),
            },
            false,
        )
        .unwrap();
    assert_eq!(
        reopened
            .subagent_history("task_1", &subagent_id)
            .unwrap()
            .revision,
        4
    );
}

#[test]
fn the_sweep_upgrades_a_large_legacy_journal_nobody_opens() {
    let temp = tempdir().unwrap();
    // Four frames of a 300 KB message repeat it ten times: over the journal budget.
    let (store, subagent_id, history_dir) = legacy_subagent(temp.path(), 4, 300_000);
    let snapshot_len = std::fs::metadata(history_dir.join("history.snapshot"))
        .unwrap()
        .len();
    assert!(journal_len(&history_dir) > super::super::history::journal_budget(snapshot_len));

    sweep(
        &Arc::downgrade(&store.inner),
        &temp.path().join("task-store-v1/tasks"),
    );

    assert_eq!(journal_len(&history_dir), 0);
    assert_last_frame_history(&store, &subagent_id, 4);
}

#[test]
fn the_sweep_leaves_small_journals_unread_and_stops_without_the_store() {
    let temp = tempdir().unwrap();
    let tasks_root = temp.path().join("task-store-v1/tasks");
    let (store, _, small_dir) = legacy_subagent(temp.path(), 2, 16);
    let small = journal_len(&small_dir);

    sweep(&Arc::downgrade(&store.inner), &tasks_root);
    assert_eq!(
        journal_len(&small_dir),
        small,
        "below the threshold the load decides"
    );

    drop(store);
    let large_root = tempdir().unwrap();
    let (store, _, large_dir) = legacy_subagent(large_root.path(), 4, 300_000);
    let large = journal_len(&large_dir);
    drop(store);
    sweep(&Weak::new(), &large_root.path().join("task-store-v1/tasks"));
    assert_eq!(
        journal_len(&large_dir),
        large,
        "a dropped Store is never reopened"
    );
}
