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

#[test]
fn stored_references_gain_their_image_size_when_the_task_opens() {
    let root = TempDir::new().unwrap();
    let task_id = "task_image_sizes";
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
    let user_image =
        chat_images::store(&task_dir, &crate::media::tests::encoded_png(4, 3)).unwrap();
    let agent_image =
        chat_images::store(&task_dir, &crate::media::tests::encoded_png(2, 5)).unwrap();
    // References as the first release of ADR-0075 stored them: no width or height.
    let snapshot_path = task_dir.join("chat.snapshot");
    let mut released: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&snapshot_path).unwrap()).unwrap();
    released["messages"] = serde_json::json!([
        {
            "sequence": 1,
            "chat": {
                "cursor": "1",
                "identity": "user-1",
                "message_type": "user",
                "message_id": "user-1",
                "message": {
                    "kind": "user",
                    "id": "user-1",
                    "text": "",
                    "created_at": "0",
                    "attachments": [{
                        "kind": "image",
                        "label": "shot.png",
                        "payload": { "imageId": user_image, "mimeType": "image/png", "sizeBytes": 1 }
                    }]
                }
            }
        },
        {
            "sequence": 2,
            "chat": {
                "cursor": "2",
                "identity": "agent-1",
                "message_type": "agent_message",
                "message_id": "agent-1",
                "message": {
                    "kind": "agent_message",
                    "id": "agent-1",
                    "role": "agent",
                    "created_at": "0",
                    "parts": [{
                        "kind": "image",
                        "media_type": "image/png",
                        "image_id": agent_image,
                        "size_bytes": 1
                    }]
                }
            }
        }
    ]);
    std::fs::write(&snapshot_path, serde_json::to_vec(&released).unwrap()).unwrap();

    let (store, _commits) = TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    let loaded = store.load(task_id).expect("stored references load");

    let NormalizedMessage::User { attachments, .. } = &loaded.messages[0].chat.message else {
        panic!("expected the user message");
    };
    assert_eq!(
        chat_images::attachment_dimensions(&attachments[0]),
        Some((4, 3))
    );
    let NormalizedMessage::AgentMessage { parts, .. } = &loaded.messages[1].chat.message else {
        panic!("expected the agent message");
    };
    assert!(matches!(
        parts[0],
        AgentMessagePart::Image {
            width: Some(2),
            height: Some(5),
            ..
        }
    ));
    // The sizes are durable, so the next open reads no image header.
    let metadata: serde_json::Value =
        serde_json::from_slice(&std::fs::read(task_dir.join("task.json")).unwrap()).unwrap();
    let snapshot = metadata["chatSnapshot"].as_str().unwrap();
    let published: serde_json::Value =
        serde_json::from_slice(&std::fs::read(task_dir.join(snapshot)).unwrap()).unwrap();
    assert!(!chat_images::has_references_without_dimensions(
        &serde_json::from_value::<Vec<crate::storage::records::StoredMessage>>(
            published["messages"].clone()
        )
        .unwrap()
    ));
    store.shutdown().unwrap();
}
