use tempfile::tempdir;

use crate::protocol::model::{AgentMessagePart, AgentMessageRole, NormalizedMessage};
use crate::storage::Store;

use super::{SubagentCapabilitiesRecord, SubagentStatusRecord};

#[test]
fn streamed_child_text_writes_only_the_new_output_and_survives_reopen() {
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
    let seed = "x".repeat(1024 * 1024);
    store
        .append_subagent_message_part("task_1", "child-session", child_text(&seed))
        .unwrap();
    let journal = temp
        .path()
        .join("task-store-v1/tasks/task_1/subagents")
        .join(&record.subagent_id)
        .join("history.journal");
    let before = std::fs::metadata(&journal).unwrap().len();
    for _ in 0..31 {
        store
            .append_subagent_message_part("task_1", "child-session", child_text(&"y".repeat(256)))
            .unwrap();
    }
    let written = std::fs::metadata(&journal).unwrap().len() - before;
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
fn previous_child_journal_accepts_mixed_updates_and_compacts_without_losing_history() {
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
    let history_dir = temp
        .path()
        .join("task-store-v1/tasks/task_1/subagents")
        .join(&record.subagent_id);
    drop(store);
    std::fs::write(
        history_dir.join("history.journal"),
        include_bytes!("fixtures/subagent-history-v1.journal"),
    )
    .unwrap();

    let reopened = Store::open(temp.path().to_path_buf()).unwrap();
    let journal_len = || {
        std::fs::metadata(history_dir.join("history.journal"))
            .unwrap()
            .len()
    };
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
                .unwrap();
            assert_eq!(journal_len(), 0, "a full change publishes the snapshot");
        }
        reopened
            .append_subagent_message_part("task_1", "child-session", child_text("!"))
            .unwrap();
    }
    assert!(
        journal_len() > 0,
        "streamed text since the snapshot is journaled"
    );
    reopened
        .append_subagent_message_part("task_1", "child-session", child_text("?"))
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
fn interrupted_subagent_compaction_keeps_history_readable_and_appendable() {
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
    let history_dir = temp
        .path()
        .join("task-store-v1/tasks/task_1/subagents")
        .join(&record.subagent_id);
    let mut old_journal = Vec::new();
    // Creating the message publishes the snapshot; 64 streamed appends follow it.
    for index in 0..65 {
        if index == 64 {
            old_journal = std::fs::read(history_dir.join("history.journal")).unwrap();
        }
        store
            .append_subagent_message_part("task_1", "child-session", child_text("x"))
            .unwrap();
    }
    drop(store);
    // Compaction published revision 65 but process death left the previous
    // journal generation in place. The snapshot already owns this prefix.
    std::fs::write(history_dir.join("history.journal"), old_journal).unwrap();
    let reopened = Store::open(temp.path().to_path_buf()).unwrap();
    assert_eq!(
        reopened
            .subagent_history("task_1", &record.subagent_id)
            .unwrap()
            .revision,
        65
    );
    reopened
        .append_subagent_message_part("task_1", "child-session", child_text("!"))
        .unwrap();
    let history = reopened
        .subagent_history("task_1", &record.subagent_id)
        .unwrap();
    assert_eq!(history.revision, 66);
    assert!(matches!(
        &history.messages[0].chat.message,
        NormalizedMessage::AgentMessage { parts, .. }
            if parts == &[AgentMessagePart::Text { text: format!("{}!", "x".repeat(65)) }]
    ));
}

#[test]
fn repeated_full_changes_keep_the_journal_from_copying_the_history() {
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
    store
        .append_subagent_message_part(
            "task_1",
            "child-session",
            child_text(&"x".repeat(1024 * 1024)),
        )
        .unwrap();
    let history_dir = temp
        .path()
        .join("task-store-v1/tasks/task_1/subagents")
        .join(&record.subagent_id);
    // Each upsert stands for a tool status update to a Subagent with a large history.
    for status in 0..8 {
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
            .unwrap();
        assert_eq!(
            std::fs::metadata(history_dir.join("history.journal"))
                .unwrap()
                .len(),
            0
        );
    }
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
    store
        .append_subagent_message_part("task_1", "child-session", child_text("accepted"))
        .unwrap();
    drop(store);
    let path = temp
        .path()
        .join("task-store-v1/tasks/task_1/subagents")
        .join(&record.subagent_id)
        .join("history.journal");
    std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(b"{\"revision\":2,\"appendText\":{\"identity\":\"message-1\",\"role\":\"agent\",\"text\":\"uncommitted")
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
    let record = store
        .record_subagent_spawn(
            "task_1",
            "root-session",
            "child-session",
            "Researcher".to_string(),
            "".to_string(),
            SubagentCapabilitiesRecord::default(),
            Vec::new(),
        )
        .unwrap();
    store
        .append_subagent_message(
            "task_1",
            "child-session",
            NormalizedMessage::User {
                id: "acp:child-session:user:collab:root-message:prompt".to_string(),
                text: "Launch a subagent".to_string(),
                created_at: "2026-08-27T00:00:00Z".to_string(),
                attachments: Vec::new(),
            },
            false,
        )
        .unwrap();
    store
        .append_subagent_message(
            "task_1",
            "child-session",
            NormalizedMessage::Activity {
                id: format!("{}:codex:started", record.subagent_id),
                title: "Main Agent started this subagent".to_string(),
                status: crate::protocol::model::ActivityStatus::Completed,
                created_at: "2026-08-27T00:00:00Z".to_string(),
                collapsed: true,
                steps: vec![crate::protocol::model::ActivityStep::Text {
                    text: "Prompt unavailable".to_string(),
                    level: Some("info".to_string()),
                }],
            },
            false,
        )
        .unwrap();

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
    store
        .append_subagent_message_part("task_1", "child-session", child_text("Rendered:"))
        .unwrap();
    let task_dir = temp.path().join("task-store-v1/tasks/task_1");
    let history_dir = task_dir.join("subagents").join(&record.subagent_id);
    // A history written before ADR-0075 kept the image bytes inline.
    let snapshot_path = history_dir.join("history.snapshot");
    drop(store);
    let mut history: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&snapshot_path).unwrap()).unwrap();
    let journal = std::fs::read_to_string(history_dir.join("history.journal")).unwrap();
    assert!(
        journal.is_empty(),
        "creating the message published the snapshot"
    );
    history["messages"][0]["chat"]["message"]["parts"]
        .as_array_mut()
        .unwrap()
        .push(
            serde_json::json!({ "kind": "image", "media_type": "image/png", "data": "aW1hZ2U=" }),
        );
    std::fs::write(&snapshot_path, serde_json::to_vec(&history).unwrap()).unwrap();

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
    let published = std::fs::read_to_string(&snapshot_path).unwrap();
    assert!(
        !published.contains("aW1hZ2U="),
        "the migrated history is durable"
    );
}

#[test]
fn a_subagent_image_reference_gains_its_size_on_load() {
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
    store
        .append_subagent_message_part("task_1", "child-session", child_text("Rendered:"))
        .unwrap();
    drop(store);
    let task_dir = temp.path().join("task-store-v1/tasks/task_1");
    let image_id =
        crate::storage::chat_images::store(&task_dir, &crate::media::tests::encoded_png(6, 2))
            .unwrap();
    // A reference as the first release of ADR-0075 stored it: no width or height.
    let snapshot_path = task_dir
        .join("subagents")
        .join(&record.subagent_id)
        .join("history.snapshot");
    let mut history: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&snapshot_path).unwrap()).unwrap();
    history["messages"][0]["chat"]["message"]["parts"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "kind": "image",
            "media_type": "image/png",
            "image_id": image_id,
            "size_bytes": 1,
        }));
    std::fs::write(&snapshot_path, serde_json::to_vec(&history).unwrap()).unwrap();

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
        serde_json::from_slice(&std::fs::read(&snapshot_path).unwrap()).unwrap();
    assert_eq!(
        published["messages"][0]["chat"]["message"]["parts"][1]["width"], 6,
        "the size is durable"
    );
}
