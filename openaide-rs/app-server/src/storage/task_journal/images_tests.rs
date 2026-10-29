use tempfile::TempDir;

use super::super::store::TaskJournalStore;
use super::super::TaskWrite;
use crate::protocol::model::{AgentMessagePart, NormalizedMessage};
use crate::storage::chat_images;

/// A Chat written by the previous release (schema 2) with an Image attachment,
/// the retired inline text syntax, and an Agent image part.
fn released_messages_v2() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/chat-messages-inline-images-v2.json"))
        .expect("released v2 Chat fixture")
}

#[test]
fn released_inline_images_move_to_files_when_the_task_opens() {
    let root = TempDir::new().unwrap();
    let task_id = "task_inline_images";
    let (store, _commits) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    store
        .submit(TaskWrite::barrier_create(
            super::super::store::tests::task_projection(task_id),
        ))
        .unwrap()
        .wait()
        .unwrap();
    store.shutdown().unwrap();
    let task_dir = root.path().join("task-store-v1/tasks").join(task_id);
    let snapshot_path = task_dir.join("chat.snapshot");
    let mut released: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&snapshot_path).unwrap()).unwrap();
    released["schemaVersion"] = serde_json::json!(2);
    released["messages"] = released_messages_v2();
    std::fs::write(&snapshot_path, serde_json::to_vec(&released).unwrap()).unwrap();

    let (store, _commits) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    let loaded = store.load(task_id).expect("released Chat migrates");

    let NormalizedMessage::User {
        text, attachments, ..
    } = &loaded.messages[0].chat.message
    else {
        panic!("expected the user message");
    };
    assert_eq!(text, "Compare  with this");
    let stored: Vec<_> = attachments
        .iter()
        .map(|attachment| {
            let image_id = chat_images::attachment_image_id(attachment).unwrap();
            chat_images::read(&task_dir, &image_id).unwrap().unwrap()
        })
        .collect();
    assert_eq!(stored, vec![b"image".to_vec(), b"legacy".to_vec()]);
    let NormalizedMessage::AgentMessage { parts, .. } = &loaded.messages[1].chat.message else {
        panic!("expected the agent message");
    };
    let AgentMessagePart::Image {
        media_type,
        data,
        image_id: Some(image_id),
        size_bytes: Some(5),
        ..
    } = &parts[1]
    else {
        panic!("expected an Agent image reference");
    };
    assert_eq!((media_type.as_str(), data.as_str()), ("image/webp", ""));
    assert_eq!(
        chat_images::read(&task_dir, image_id).unwrap().unwrap(),
        b"agent"
    );

    // The migrated generation is durable: it holds references only, at schema 3.
    let metadata: serde_json::Value =
        serde_json::from_slice(&std::fs::read(task_dir.join("task.json")).unwrap()).unwrap();
    let snapshot = metadata["chatSnapshot"].as_str().unwrap();
    let published = std::fs::read_to_string(task_dir.join(snapshot)).unwrap();
    assert!(published.contains("\"schemaVersion\":3"));
    assert!(!published.contains("aW1hZ2U=") && !published.contains("YWdlbnQ="));
    store.shutdown().unwrap();
}

#[test]
fn a_sent_image_is_stored_before_its_frame_and_the_frame_holds_a_reference() {
    let root = TempDir::new().unwrap();
    let task_id = "task_sent_image";
    let (store, _commits) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    let mut projection = super::super::store::tests::task_projection(task_id);
    let mut messages: Vec<crate::storage::records::StoredMessage> =
        serde_json::from_value(released_messages_v2()).unwrap();
    projection.messages.push(messages.remove(1));
    store
        .submit(TaskWrite::barrier_create(projection))
        .unwrap()
        .wait()
        .unwrap();
    store.shutdown().unwrap();

    let task_dir = root.path().join("task-store-v1/tasks").join(task_id);
    let mut journal = Vec::new();
    for entry in std::fs::read_dir(&task_dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            journal.extend(std::fs::read(path).unwrap());
        }
    }
    let journal = String::from_utf8_lossy(&journal);
    assert!(
        !journal.contains("YWdlbnQ="),
        "no Task file carries image bytes"
    );
    let images = std::fs::read_dir(task_dir.join(chat_images::IMAGES_DIR))
        .unwrap()
        .count();
    assert_eq!(images, 1);
}
