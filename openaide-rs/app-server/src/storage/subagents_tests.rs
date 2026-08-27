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
