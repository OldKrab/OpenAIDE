use tempfile::tempdir;

use crate::protocol::model::{AgentMessagePart, AgentMessageRole, NormalizedMessage};
use crate::storage::Store;

use super::{stream, SubagentCapabilitiesRecord, SubagentStatusRecord};

#[test]
fn streamed_child_text_writes_only_the_new_output_and_survives_reopen() {
    let temp = tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let record = spawn_researcher(&store);
    let seed = "x".repeat(1024 * 1024);
    store
        .append_subagent_message_part("task_1", "child-session", child_text(&seed))
        .unwrap()
        .wait()
        .unwrap();
    let dir = history_dir(temp.path(), &record.subagent_id);
    let before = journal_len(&dir);
    for _ in 0..31 {
        store
            .append_subagent_message_part("task_1", "child-session", child_text(&"y".repeat(256)))
            .unwrap()
            .wait()
            .unwrap();
    }
    let written = journal_len(&dir) - before;
    assert!(written < 32 * 1024, "8KiB of output wrote {written} bytes");
    drop(store);
    let reopened = Store::open(temp.path().to_path_buf()).unwrap();
    let history = reopened
        .subagent_history("task_1", &record.subagent_id)
        .unwrap();
    assert_eq!(history.revision, 32);
    assert!(matches!(
        &history.messages[0].chat.message,
        NormalizedMessage::AgentMessage { parts, .. }
            if parts == &[AgentMessagePart::Text { text: format!("{seed}{}", "y".repeat(31 * 256)) }]
    ));
}

#[test]
fn a_schema_one_history_migrates_to_a_stream_that_accepts_mixed_updates() {
    let temp = tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let record = spawn_researcher(&store);
    drop(store);
    let dir = history_dir(temp.path(), &record.subagent_id);
    let mut snapshot: serde_json::Value =
        serde_json::from_slice(include_bytes!("fixtures/subagent-history-v2.snapshot")).unwrap();
    snapshot["schemaVersion"] = 1.into();
    write_legacy_history(
        &dir,
        &serde_json::to_vec(&snapshot).unwrap(),
        include_bytes!("fixtures/subagent-history-v1.journal"),
    );

    let reopened = Store::open(temp.path().to_path_buf()).unwrap();
    for index in 0..62 {
        if index == 31 {
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
                .unwrap()
                .wait()
                .unwrap();
        }
        reopened
            .append_subagent_message_part("task_1", "child-session", child_text("!"))
            .unwrap()
            .wait()
            .unwrap();
    }
    assert!(!dir.join("history.snapshot").exists());
    assert!(!dir.join("history.journal").exists());
    assert_eq!(stream::journaled_operations(&dir).len(), 63);
    reopened
        .append_subagent_message_part("task_1", "child-session", child_text("?"))
        .unwrap()
        .wait()
        .unwrap();
    drop(reopened);

    let final_store = Store::open(temp.path().to_path_buf()).unwrap();
    let history = final_store
        .subagent_history("task_1", &record.subagent_id)
        .unwrap();
    assert_eq!(history.revision, 65);
    assert_eq!(history.messages.len(), 2);
    assert!(matches!(
        &history.messages[0].chat.message,
        NormalizedMessage::AgentMessage { parts, created_at, .. }
            if parts == &[AgentMessagePart::Text { text: format!("Hello{}?", "!".repeat(62)) }]
                && created_at == "1"
    ));
    assert!(matches!(
        &history.messages[1].chat.message,
        NormalizedMessage::User { text, .. } if text == "Check the result"
    ));
}

#[test]
fn a_released_schema_two_history_migrates_once_with_its_messages_and_plan() {
    let temp = tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let record = spawn_researcher(&store);
    drop(store);
    let dir = history_dir(temp.path(), &record.subagent_id);
    write_legacy_history(
        &dir,
        include_bytes!("fixtures/subagent-history-v2.snapshot"),
        include_bytes!("fixtures/subagent-history-v2.journal"),
    );
    let catalog_path = dir.parent().unwrap().join("catalog.json");
    let catalog_schema = || {
        let catalog: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&catalog_path).unwrap()).unwrap();
        catalog["schemaVersion"].as_u64().unwrap()
    };
    // As an App Server that wrote only ADR-0046 histories left the catalog.
    let mut catalog: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&catalog_path).unwrap()).unwrap();
    catalog["schemaVersion"] = 1.into();
    std::fs::write(&catalog_path, serde_json::to_vec(&catalog).unwrap()).unwrap();

    let reopened = Store::open(temp.path().to_path_buf()).unwrap();
    let history = reopened
        .subagent_history("task_1", &record.subagent_id)
        .unwrap();

    assert_eq!(history.revision, 4);
    assert!(matches!(
        &history.messages[..],
        [agent, user]
            if matches!(&agent.chat.message, NormalizedMessage::AgentMessage { parts, .. }
                if parts == &[AgentMessagePart::Text { text: "Hello world".to_string() }])
            && matches!(&user.chat.message, NormalizedMessage::User { text, .. }
                if text == "Check the result")
    ));
    assert_eq!(
        history.current_plan.unwrap().entries[0].content,
        "Run the suite"
    );
    assert_eq!(
        catalog_schema(),
        2,
        "an older App Server rejects the Task's catalog"
    );
    assert!(dir.join("stream.json").is_file());
    assert!(!dir.join("history.snapshot").exists());
    assert!(!dir.join("history.journal").exists());
    let snapshot = std::fs::read(stream::snapshot_path(&dir)).unwrap();
    reopened
        .append_subagent_message_part("task_1", "child-session", child_text("!"))
        .unwrap()
        .wait()
        .unwrap();
    drop(reopened);

    let reopened = Store::open(temp.path().to_path_buf()).unwrap();
    let history = reopened
        .subagent_history("task_1", &record.subagent_id)
        .unwrap();
    assert_eq!(history.revision, 5);
    assert!(matches!(
        &history.messages[0].chat.message,
        NormalizedMessage::AgentMessage { parts, .. }
            if parts == &[AgentMessagePart::Text { text: "Hello world!".to_string() }]
    ));
    assert_eq!(
        std::fs::read(stream::snapshot_path(&dir)).unwrap(),
        snapshot,
        "the migrated history is not migrated again"
    );
}

#[test]
fn interrupted_compaction_files_are_ignored_and_removed_by_the_next_compaction() {
    let temp = tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let record = spawn_researcher(&store);
    store
        .append_subagent_message_part("task_1", "child-session", child_text("x"))
        .unwrap()
        .wait()
        .unwrap();
    drop(store);
    // A crash between writing a generation and publishing it, or between
    // publishing it and removing the previous one, leaves unnamed files.
    let dir = history_dir(temp.path(), &record.subagent_id);
    let stale = ["chat.snapshot.stale", "chat.journal.stale"];
    for name in stale {
        std::fs::write(dir.join(name), b"not a generation").unwrap();
    }

    let reopened = Store::open(temp.path().to_path_buf()).unwrap();
    assert_eq!(
        reopened
            .subagent_history("task_1", &record.subagent_id)
            .unwrap()
            .revision,
        1
    );
    for _ in 1..stream::COMPACT_AFTER_FRAMES {
        reopened
            .append_subagent_message_part("task_1", "child-session", child_text("x"))
            .unwrap()
            .wait()
            .unwrap();
    }
    assert!(
        stream::journaled_operations(&dir).is_empty(),
        "the full journal became a new generation"
    );
    for name in stale {
        assert!(!dir.join(name).exists(), "{name} is removed");
    }
    drop(reopened);
    let history = Store::open(temp.path().to_path_buf())
        .unwrap()
        .subagent_history("task_1", &record.subagent_id)
        .unwrap();
    assert_eq!(history.revision, stream::COMPACT_AFTER_FRAMES);
    assert!(matches!(
        &history.messages[0].chat.message,
        NormalizedMessage::AgentMessage { parts, .. }
            if parts == &[AgentMessagePart::Text {
                text: "x".repeat(stream::COMPACT_AFTER_FRAMES as usize),
            }]
    ));
}

#[test]
fn a_message_update_journals_only_that_message() {
    let temp = tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let record = spawn_researcher(&store);
    store
        .append_subagent_message_part(
            "task_1",
            "child-session",
            child_text(&"x".repeat(1024 * 1024)),
        )
        .unwrap()
        .wait()
        .unwrap();
    let dir = history_dir(temp.path(), &record.subagent_id);
    let snapshot = std::fs::read(stream::snapshot_path(&dir)).unwrap();
    // Each upsert stands for a tool status update to a Subagent with a large history.
    for status in 0..8 {
        let before = journal_len(&dir);
        store
            .append_subagent_message(
                "task_1",
                "child-session",
                NormalizedMessage::User {
                    id: "user-message".to_string(),
                    text: format!("status {status}"),
                    created_at: "2".to_string(),
                    attachments: Vec::new(),
                },
                true,
            )
            .unwrap()
            .wait()
            .unwrap();
        let written = journal_len(&dir) - before;
        assert!(written < 4096, "a small update wrote {written} bytes");
    }
    assert_eq!(
        std::fs::read(stream::snapshot_path(&dir)).unwrap(),
        snapshot,
        "the large history is not rewritten"
    );
    drop(store);

    let reopened = Store::open(temp.path().to_path_buf()).unwrap();
    let history = reopened
        .subagent_history("task_1", &record.subagent_id)
        .unwrap();
    assert_eq!(history.revision, 9);
    assert_eq!(history.messages.len(), 2);
    assert!(matches!(
        &history.messages[1].chat.message,
        NormalizedMessage::User { text, .. } if text == "status 7"
    ));
}

/// Replaces a history's files with an ADR-0046 snapshot and journal.
fn write_legacy_history(dir: &std::path::Path, snapshot: &[u8], journal: &[u8]) {
    for entry in std::fs::read_dir(dir).unwrap() {
        std::fs::remove_file(entry.unwrap().path()).unwrap();
    }
    std::fs::write(dir.join("history.snapshot"), snapshot).unwrap();
    std::fs::write(dir.join("history.journal"), journal).unwrap();
}

/// An ADR-0046 schema 2 snapshot holding `messages` at revision 1.
fn legacy_snapshot(messages: serde_json::Value) -> Vec<u8> {
    let count = messages.as_array().unwrap().len();
    serde_json::to_vec(&serde_json::json!({
        "schemaVersion": 2,
        "revision": 1,
        "journalFrames": 0,
        "messages": messages,
        "messageMeta": {
            "task_id": "task_1",
            "version": 1,
            "message_count": count,
            "local_history_updated_at": "1",
            "first_cursor": "m:1",
            "last_cursor": format!("m:{count}"),
        },
        "currentPlan": null,
    }))
    .unwrap()
}

/// A stored child reply as an ADR-0046 snapshot holds it.
fn legacy_child_reply(parts: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "sequence": 1,
        "chat": {
            "cursor": "m:1",
            "identity": "message-1",
            "message_type": "agent_message",
            "message_id": "message-1",
            "message": {
                "kind": "agent_message",
                "id": "message-1",
                "role": "agent",
                "parts": parts,
                "created_at": "1",
            },
        },
    })
}

fn journal_len(dir: &std::path::Path) -> u64 {
    std::fs::metadata(stream::journal_path(dir)).map_or(0, |metadata| metadata.len())
}

fn child_text(text: &str) -> NormalizedMessage {
    NormalizedMessage::AgentMessage {
        id: "message-1".to_string(),
        role: AgentMessageRole::Agent,
        parts: vec![AgentMessagePart::Text {
            text: text.to_string(),
        }],
        created_at: "2026-09-05T00:00:00Z".to_string(),
    }
}

#[test]
fn incomplete_subagent_append_preserves_accepted_history_and_allows_new_output() {
    use std::io::Write;

    let temp = tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let record = spawn_researcher(&store);
    store
        .append_subagent_message_part("task_1", "child-session", child_text("accepted"))
        .unwrap()
        .wait()
        .unwrap();
    drop(store);
    // A frame length announcing more payload than the crash left behind.
    let path = stream::journal_path(&history_dir(temp.path(), &record.subagent_id));
    let mut torn = 64_u64.to_le_bytes().to_vec();
    torn.extend_from_slice(b"{\"format_version\":1");
    std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(&torn)
        .unwrap();

    let reopened = Store::open(temp.path().to_path_buf()).unwrap();
    assert_eq!(
        reopened
            .subagent_history("task_1", &record.subagent_id)
            .unwrap()
            .revision,
        1
    );
    reopened
        .append_subagent_message_part("task_1", "child-session", child_text("!"))
        .unwrap()
        .wait()
        .unwrap();
    let history = reopened
        .subagent_history("task_1", &record.subagent_id)
        .unwrap();
    assert_eq!(history.revision, 2);
    assert!(matches!(
        &history.messages[0].chat.message,
        NormalizedMessage::AgentMessage { parts, .. }
            if parts == &[AgentMessagePart::Text { text: "accepted!".to_string() }]
    ));
}

#[test]
fn nested_catalog_and_independent_history_survive_reopen() {
    let temp = tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let root = store
        .record_subagent_spawn(
            "task_1",
            "root-session",
            "child-session",
            "Researcher".to_string(),
            "Inspect the protocol".to_string(),
            SubagentCapabilitiesRecord::default(),
            Vec::new(),
        )
        .unwrap();
    let nested = store
        .record_subagent_spawn(
            "task_1",
            "child-session",
            "nested-session",
            "Verifier".to_string(),
            "Check the result".to_string(),
            SubagentCapabilitiesRecord::default(),
            Vec::new(),
        )
        .unwrap();
    assert_eq!(
        nested.parent_subagent_id.as_deref(),
        Some(root.subagent_id.as_str())
    );

    for text in ["Hello", " world"] {
        store
            .append_subagent_message_part(
                "task_1",
                "child-session",
                NormalizedMessage::AgentMessage {
                    id: "message-1".to_string(),
                    role: AgentMessageRole::Agent,
                    parts: vec![AgentMessagePart::Text {
                        text: text.to_string(),
                    }],
                    created_at: "2026-08-27T00:00:00Z".to_string(),
                },
            )
            .unwrap()
            .wait()
            .unwrap();
    }
    store
        .update_subagent_status("task_1", "child-session", SubagentStatusRecord::Completed)
        .unwrap();
    drop(store);

    let reopened = Store::open(temp.path().to_path_buf()).unwrap();
    let catalog = reopened.subagent_catalog("task_1").unwrap();
    assert_eq!(catalog.entries.len(), 2);
    assert_eq!(catalog.entries[0].status, SubagentStatusRecord::Completed);
    let history = reopened
        .subagent_history("task_1", &root.subagent_id)
        .unwrap();
    assert_eq!(history.messages.len(), 1);
    let NormalizedMessage::AgentMessage { parts, .. } = &history.messages[0].chat.message else {
        panic!("expected Agent message")
    };
    assert_eq!(
        parts,
        &[AgentMessagePart::Text {
            text: "Hello world".to_string()
        }]
    );
}

#[test]
fn duplicate_native_spawn_reconciles_to_one_product_identity() {
    let temp = tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let first = store
        .record_subagent_spawn(
            "task_1",
            "root-session",
            "child-session",
            "One".to_string(),
            "First".to_string(),
            SubagentCapabilitiesRecord::default(),
            Vec::new(),
        )
        .unwrap();
    let duplicate = store
        .record_subagent_spawn(
            "task_1",
            "root-session",
            "child-session",
            "Changed".to_string(),
            "Changed".to_string(),
            SubagentCapabilitiesRecord::default(),
            Vec::new(),
        )
        .unwrap();
    assert_eq!(duplicate.subagent_id, first.subagent_id);
    assert_eq!(store.subagent_catalog("task_1").unwrap().entries.len(), 1);
}

#[test]
fn subagent_user_chunks_form_one_prompt_in_history() {
    let temp = tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
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

    for text in ["Check the ", "second failure."] {
        store
            .append_subagent_message_part(
                "task_1",
                "child-session",
                NormalizedMessage::User {
                    id: "child-user-1".to_string(),
                    text: text.to_string(),
                    created_at: "2026-08-27T00:00:00Z".to_string(),
                    attachments: Vec::new(),
                },
            )
            .unwrap()
            .wait()
            .unwrap();
    }

    let history = store
        .subagent_history("task_1", &record.subagent_id)
        .unwrap();
    assert!(matches!(
        history.messages.as_slice(),
        [stored] if matches!(
            &stored.chat.message,
            NormalizedMessage::User { text, .. } if text == "Check the second failure."
        )
    ));
}

#[test]
fn legacy_codex_causal_root_prompt_is_hidden_from_child_history() {
    let temp = tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let record = spawn_researcher(&store);
    drop(store);
    let identity = format!("{}:codex:started", record.subagent_id);
    // What an older codex-acp and App Server left in an ADR-0046 history.
    write_legacy_history(
        &history_dir(temp.path(), &record.subagent_id),
        &legacy_snapshot(serde_json::json!([
            {
                "sequence": 1,
                "chat": {
                    "cursor": "m:1",
                    "identity": "acp:child-session:user:collab:root-message:prompt",
                    "message_type": "user",
                    "message_id": "acp:child-session:user:collab:root-message:prompt",
                    "message": {
                        "kind": "user",
                        "id": "acp:child-session:user:collab:root-message:prompt",
                        "text": "Launch a subagent",
                        "created_at": "2026-08-27T00:00:00Z",
                    },
                },
            },
            {
                "sequence": 2,
                "chat": {
                    "cursor": "m:2",
                    "identity": identity,
                    "message_type": "activity",
                    "message_id": identity,
                    "message": {
                        "kind": "activity",
                        "id": identity,
                        "title": "Main Agent started this subagent",
                        "status": "completed",
                        "created_at": "2026-08-27T00:00:00Z",
                        "collapsed": true,
                        "steps": [{ "kind": "text", "text": "Prompt unavailable", "level": "info" }],
                    },
                },
            },
        ])),
        b"",
    );

    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let history = store
        .subagent_history("task_1", &record.subagent_id)
        .unwrap();
    assert!(matches!(
        history.messages.as_slice(),
        [stored] if matches!(
            &stored.chat.message,
            NormalizedMessage::Activity { steps, .. }
                if matches!(steps.as_slice(), [crate::protocol::model::ActivityStep::Text { level: Some(level), .. }]
                    if level == "agent_boundary")
        )
    ));
}

#[test]
fn subagent_history_pages_before_the_selected_cursor() {
    let temp = tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
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
    for index in 1..=3 {
        store
            .append_subagent_message(
                "task_1",
                "child-session",
                NormalizedMessage::AgentMessage {
                    id: format!("message-{index}"),
                    role: AgentMessageRole::Agent,
                    parts: vec![AgentMessagePart::Text {
                        text: format!("message {index}"),
                    }],
                    created_at: "2026-08-27T00:00:00Z".to_string(),
                },
                false,
            )
            .unwrap()
            .wait()
            .unwrap();
    }
    let history = store
        .subagent_history("task_1", &record.subagent_id)
        .unwrap();
    let before = history.messages[2].chat.cursor.clone();

    let page = store
        .subagent_page_before("task_1", &record.subagent_id, &before, 1)
        .unwrap();

    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].message_id, "message-2");
    assert!(page.has_before);
    assert_eq!(page.total_count, 3);
}

#[test]
fn a_subagent_history_with_inline_images_moves_them_to_task_files_on_load() {
    let temp = tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let record = spawn_researcher(&store);
    drop(store);
    let dir = history_dir(temp.path(), &record.subagent_id);
    // A history written before ADR-0075 kept the image bytes inline.
    write_legacy_history(
        &dir,
        &legacy_snapshot(serde_json::json!([legacy_child_reply(serde_json::json!([
            { "kind": "text", "text": "Rendered:" },
            { "kind": "image", "media_type": "image/png", "data": "aW1hZ2U=" },
        ]))])),
        b"",
    );

    let reopened = Store::open(temp.path().to_path_buf()).unwrap();
    let loaded = reopened
        .subagent_history("task_1", &record.subagent_id)
        .unwrap();

    let NormalizedMessage::AgentMessage { parts, .. } = &loaded.messages[0].chat.message else {
        panic!("expected the child reply");
    };
    let AgentMessagePart::Image {
        data,
        image_id: Some(image_id),
        ..
    } = &parts[1]
    else {
        panic!("expected an image reference");
    };
    assert!(data.is_empty());
    assert_eq!(
        reopened.read_chat_image("task_1", image_id).unwrap(),
        Some(b"image".to_vec())
    );
    let published = std::fs::read_to_string(stream::snapshot_path(&dir)).unwrap();
    assert!(
        !published.contains("aW1hZ2U="),
        "the migrated history is durable"
    );
}

#[test]
fn a_subagent_image_reference_gains_its_size_on_load() {
    let temp = tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let record = spawn_researcher(&store);
    drop(store);
    let task_dir = temp.path().join("task-store-v1/tasks/task_1");
    let image_id =
        crate::storage::chat_images::store(&task_dir, &crate::media::tests::encoded_png(6, 2))
            .unwrap();
    // A reference as the first release of ADR-0075 stored it: no width or height.
    let dir = history_dir(temp.path(), &record.subagent_id);
    write_legacy_history(
        &dir,
        &legacy_snapshot(serde_json::json!([legacy_child_reply(serde_json::json!([
            { "kind": "text", "text": "Rendered:" },
            { "kind": "image", "media_type": "image/png", "image_id": image_id, "size_bytes": 1 },
        ]))])),
        b"",
    );

    let reopened = Store::open(temp.path().to_path_buf()).unwrap();
    let loaded = reopened
        .subagent_history("task_1", &record.subagent_id)
        .unwrap();

    let NormalizedMessage::AgentMessage { parts, .. } = &loaded.messages[0].chat.message else {
        panic!("expected the child reply");
    };
    assert!(matches!(
        parts[1],
        AgentMessagePart::Image {
            width: Some(6),
            height: Some(2),
            ..
        }
    ));
    let published: serde_json::Value =
        serde_json::from_slice(&std::fs::read(stream::snapshot_path(&dir)).unwrap()).unwrap();
    assert_eq!(
        published["messages"][0]["chat"]["message"]["parts"][1]["width"], 6,
        "the size is durable"
    );
}

fn spawn_researcher(store: &Store) -> super::SubagentRecord {
    store
        .record_subagent_spawn(
            "task_1",
            "root-session",
            "child-session",
            "Researcher".to_string(),
            "Inspect".to_string(),
            SubagentCapabilitiesRecord::default(),
            Vec::new(),
        )
        .unwrap()
}

fn history_dir(root: &std::path::Path, subagent_id: &str) -> std::path::PathBuf {
    root.join("task-store-v1/tasks/task_1/subagents")
        .join(subagent_id)
}

#[test]
fn plan_and_permission_changes_are_journaled_and_survive_reopen() {
    use crate::protocol::model::{
        ActivityStatus, ActivityStep, AgentPlan, AgentPlanEntry, AgentPlanPriority,
        AgentPlanStatus, ToolPermissionDecision, ToolPermissionOutcome,
    };

    let temp = tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let record = spawn_researcher(&store);
    store
        .append_subagent_message(
            "task_1",
            "child-session",
            NormalizedMessage::Activity {
                id: "activity-1".to_string(),
                title: "Run tests".to_string(),
                status: ActivityStatus::Running,
                created_at: "1".to_string(),
                collapsed: false,
                steps: vec![ActivityStep::Tool {
                    tool_call_id: Some("call-1".to_string()),
                    name: "execute".to_string(),
                    status: ActivityStatus::Running,
                    presentation: None,
                    description: None,
                    input_summary: None,
                    output_preview: None,
                    detail_artifact_id: None,
                    details: None,
                    permission_outcomes: Vec::new(),
                    background_outcome: None,
                    pending: true,
                }],
            },
            false,
        )
        .unwrap()
        .wait()
        .unwrap();
    let plan = AgentPlan {
        entries: vec![AgentPlanEntry {
            content: "Run the suite".to_string(),
            priority: AgentPlanPriority::High,
            status: AgentPlanStatus::InProgress,
        }],
    };
    store
        .set_subagent_plan("task_1", "child-session", plan.clone())
        .unwrap()
        .wait()
        .unwrap();
    store
        .record_subagent_permission_outcome(
            "task_1",
            "child-session",
            "call-1",
            ToolPermissionOutcome {
                request_id: "request-1".to_string(),
                decision: ToolPermissionDecision::Approved,
                option_id: None,
                option_label: None,
                resolved_at: "2".to_string(),
            },
        )
        .unwrap();
    let dir = history_dir(temp.path(), &record.subagent_id);
    assert_eq!(
        stream::journaled_operations(&dir),
        [["upsert_message"], ["set_plan"], ["upsert_message"]]
    );
    drop(store);

    let reopened = Store::open(temp.path().to_path_buf()).unwrap();
    let history = reopened
        .subagent_history("task_1", &record.subagent_id)
        .unwrap();
    assert_eq!(history.revision, 3);
    assert_eq!(history.current_plan, Some(plan));
    assert!(matches!(
        &history.messages[0].chat.message,
        NormalizedMessage::Activity { steps, .. }
            if matches!(&steps[..], [ActivityStep::Tool { permission_outcomes, .. }]
                if permission_outcomes.len() == 1 && permission_outcomes[0].request_id == "request-1")
    ));

    reopened
        .set_subagent_plan(
            "task_1",
            "child-session",
            AgentPlan {
                entries: Vec::new(),
            },
        )
        .unwrap()
        .wait()
        .unwrap();
    drop(reopened);
    let reopened = Store::open(temp.path().to_path_buf()).unwrap();
    assert_eq!(
        reopened
            .subagent_history("task_1", &record.subagent_id)
            .unwrap()
            .current_plan,
        None,
        "a journaled clear survives reopen"
    );
}

#[test]
fn a_newer_stream_or_catalog_schema_is_refused_unchanged() {
    let temp = tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let record = spawn_researcher(&store);
    drop(store);
    let dir = history_dir(temp.path(), &record.subagent_id);
    let bump_schema = |path: &std::path::Path| {
        let mut value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        value["schemaVersion"] = 3.into();
        std::fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
        std::fs::read(path).unwrap()
    };

    let head = bump_schema(&dir.join("stream.json"));
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let error = store
        .subagent_history("task_1", &record.subagent_id)
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("Unsupported Subagent stream schema"));
    assert!(store
        .append_subagent_message_part("task_1", "child-session", child_text("lost"))
        .unwrap()
        .wait()
        .is_err());
    assert_eq!(std::fs::read(dir.join("stream.json")).unwrap(), head);
    drop(store);

    let catalog_path = dir.parent().unwrap().join("catalog.json");
    let catalog = bump_schema(&catalog_path);
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    assert!(store.subagent_catalog("task_1").is_err());
    assert_eq!(std::fs::read(&catalog_path).unwrap(), catalog);
}

#[test]
fn a_journal_past_the_byte_threshold_starts_a_new_generation() {
    let temp = tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let record = spawn_researcher(&store);
    let dir = history_dir(temp.path(), &record.subagent_id);
    store
        .append_subagent_message_part("task_1", "child-session", child_text("small"))
        .unwrap()
        .wait()
        .unwrap();
    assert_eq!(stream::journaled_operations(&dir).len(), 1);
    let snapshot = stream::snapshot_path(&dir);

    // Past 1 MiB and larger than the snapshot it follows.
    store
        .append_subagent_message(
            "task_1",
            "child-session",
            NormalizedMessage::User {
                id: "user-message".to_string(),
                text: "x".repeat(2 << 20),
                created_at: "2".to_string(),
                attachments: Vec::new(),
            },
            false,
        )
        .unwrap()
        .wait()
        .unwrap();
    assert!(stream::journaled_operations(&dir).is_empty());
    assert!(!snapshot.exists(), "the previous generation is removed");
    drop(store);
    let history = Store::open(temp.path().to_path_buf())
        .unwrap()
        .subagent_history("task_1", &record.subagent_id)
        .unwrap();
    assert_eq!(history.revision, 2);
    assert_eq!(history.messages.len(), 2);
}
