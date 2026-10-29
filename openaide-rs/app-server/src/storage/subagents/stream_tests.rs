use tempfile::tempdir;

use super::*;
use crate::storage::records::StoredMessage;

fn chat_with_text(text: &str) -> SubagentChat {
    let mut chat = SubagentChat::default();
    chat.messages.push(StoredMessage {
        sequence: 1,
        chat: crate::protocol::model::ChatMessage {
            cursor: "m:1".to_string(),
            message_id: "message-1".to_string(),
            identity: "message-1".to_string(),
            message_type: "agent_message".to_string(),
            message: NormalizedMessage::AgentMessage {
                id: "message-1".to_string(),
                role: crate::protocol::model::AgentMessageRole::Agent,
                parts: vec![AgentMessagePart::Text {
                    text: text.to_string(),
                }],
                created_at: "1".to_string(),
            },
            timing: Default::default(),
        },
    });
    chat
}

fn append_text(dir: &Path, files: &mut StreamFiles, chat: &mut SubagentChat, text: &str) {
    let change = HistoryChange::AppendText {
        identity: "message-1".to_string(),
        text: text.to_string(),
    };
    let NormalizedMessage::AgentMessage { parts, .. } = &mut chat.messages[0].chat.message else {
        unreachable!()
    };
    let AgentMessagePart::Text { text: last } = parts.last_mut().unwrap() else {
        unreachable!()
    };
    last.push_str(text);
    advance_history_meta(chat);
    let operation = StreamOperation::describe(chat, &change).unwrap();
    chat.revision += 1;
    append(dir, files, chat, vec![operation]).unwrap();
}

#[test]
fn replay_rebuilds_the_history_its_writer_held() {
    let temp = tempdir().unwrap();
    let mut chat = chat_with_text("Hello");
    let mut files = publish_generation(temp.path(), &chat).unwrap();
    append_text(temp.path(), &mut files, &mut chat, ",");
    append_text(temp.path(), &mut files, &mut chat, " world");

    let (loaded, loaded_files) = load(temp.path()).unwrap();

    assert_eq!(
        serde_json::to_value(&loaded).unwrap(),
        serde_json::to_value(&chat).unwrap()
    );
    assert_eq!(loaded.revision, 2);
    assert_eq!(loaded_files.journal_frames(), 2);
}

#[test]
fn an_interrupted_journal_creation_is_removed_and_created_again() {
    let temp = tempdir().unwrap();
    let mut chat = chat_with_text("Hello");
    publish_generation(temp.path(), &chat).unwrap();
    // The crash left only part of the file header.
    std::fs::write(journal_path(temp.path()), b"OAIDE").unwrap();

    let (loaded, mut files) = load(temp.path()).unwrap();
    assert_eq!(loaded.revision, 0);
    assert!(!journal_path(temp.path()).exists());
    append_text(temp.path(), &mut files, &mut chat, "!");

    assert_eq!(load(temp.path()).unwrap().0.revision, 1);
}

#[test]
fn a_frame_whose_revision_disagrees_with_its_operations_is_refused() {
    let temp = tempdir().unwrap();
    let chat = chat_with_text("Hello");
    let mut files = publish_generation(temp.path(), &chat).unwrap();
    // Claims two revisions for one operation.
    let mut claimed = chat.clone();
    claimed.revision = 2;
    let operation = StreamOperation::SetPlan { plan: None };
    append(temp.path(), &mut files, &claimed, vec![operation]).unwrap();

    let error = load(temp.path()).unwrap_err();

    assert!(error.to_string().contains("revision is inconsistent"));
}
