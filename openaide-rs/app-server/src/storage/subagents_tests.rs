use tempfile::tempdir;

use crate::protocol::model::{AgentMessagePart, AgentMessageRole, NormalizedMessage};
use crate::storage::Store;

use super::{SubagentCapabilitiesRecord, SubagentStatusRecord};

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
