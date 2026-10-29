use std::collections::HashSet;
use std::time::{Duration, SystemTime};

use base64::Engine;

use super::*;
use crate::protocol::model::{AgentMessageRole, ChatMessage};

fn base64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn stored(message: NormalizedMessage) -> StoredMessage {
    StoredMessage {
        sequence: 1,
        chat: ChatMessage {
            cursor: "cursor-1".to_string(),
            identity: "message-1".to_string(),
            message_type: "user".to_string(),
            message_id: "message-1".to_string(),
            message,
            timing: Default::default(),
        },
    }
}

fn user(text: &str, attachments: Vec<Attachment>) -> StoredMessage {
    stored(NormalizedMessage::User {
        id: "user-1".to_string(),
        text: text.to_string(),
        created_at: "0".to_string(),
        attachments,
    })
}

fn image_attachment(bytes: &[u8]) -> Attachment {
    Attachment {
        kind: "image".to_string(),
        label: "shot.png".to_string(),
        path: None,
        payload: Some(serde_json::json!({
            "data": base64(bytes),
            "mimeType": "image/png",
            "sizeBytes": bytes.len(),
        })),
    }
}

fn agent_image(bytes: &[u8]) -> StoredMessage {
    stored(NormalizedMessage::AgentMessage {
        id: "agent-1".to_string(),
        role: AgentMessageRole::Agent,
        parts: vec![AgentMessagePart::Image {
            media_type: "image/png".to_string(),
            data: base64(bytes),
            image_id: None,
            size_bytes: None,
            width: None,
            height: None,
            uri: None,
        }],
        created_at: "0".to_string(),
    })
}

/// Back-dates a file past the sweep grace; nothing waits on real time.
// timing: contract — the file's recorded age, not elapsed time, is what the sweep compares.
const PAST_GRACE: Duration = UNREFERENCED_GRACE.saturating_mul(2);

fn age(path: &Path, by: Duration) {
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(SystemTime::now() - by)
        .unwrap();
}

#[test]
fn the_same_bytes_are_stored_once_under_their_hash() {
    let dir = tempfile::tempdir().unwrap();
    let first = store(dir.path(), b"png bytes").unwrap();
    let second = store(dir.path(), b"png bytes").unwrap();

    assert_eq!(first, second);
    assert!(is_image_id(&first));
    assert_eq!(read(dir.path(), &first).unwrap().unwrap(), b"png bytes");
    assert_eq!(
        fs::read_dir(dir.path().join(IMAGES_DIR)).unwrap().count(),
        1
    );
}

#[test]
fn an_id_that_is_not_a_hash_never_reaches_the_filesystem() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("task.json"), b"{}").unwrap();

    assert_eq!(read(dir.path(), "../task.json").unwrap(), None);
    assert_eq!(read(dir.path(), &"A".repeat(64)).unwrap(), None);
    assert_eq!(read(dir.path(), &"a".repeat(64)).unwrap(), None);
}

#[test]
fn user_and_agent_images_become_references() {
    let dir = tempfile::tempdir().unwrap();
    let mut messages = vec![
        user("look", vec![image_attachment(b"user image")]),
        agent_image(b"agent image"),
    ];
    assert!(has_inline_images(&messages));

    assert_eq!(externalize_messages(dir.path(), &mut messages).unwrap(), 2);

    assert!(!has_inline_images(&messages));
    let NormalizedMessage::User { attachments, .. } = &messages[0].chat.message else {
        unreachable!()
    };
    let payload = attachments[0].payload.as_ref().unwrap();
    assert!(payload.get("data").is_none());
    assert_eq!(payload["mimeType"], "image/png");
    assert_eq!(payload["sizeBytes"], 10);
    let user_id = attachment_image_id(&attachments[0]).unwrap();
    assert_eq!(read(dir.path(), &user_id).unwrap().unwrap(), b"user image");

    let NormalizedMessage::AgentMessage { parts, .. } = &messages[1].chat.message else {
        unreachable!()
    };
    let AgentMessagePart::Image {
        data,
        image_id,
        size_bytes,
        ..
    } = &parts[0]
    else {
        unreachable!()
    };
    assert!(data.is_empty());
    assert_eq!(*size_bytes, Some(11));
    let agent_id = image_id.clone().unwrap();
    assert_eq!(
        read(dir.path(), &agent_id).unwrap().unwrap(),
        b"agent image"
    );

    let mut referenced = HashSet::new();
    referenced_ids(&messages, &mut referenced);
    assert_eq!(referenced, HashSet::from([user_id, agent_id]));
    // Already-referenced Chat is left as it is.
    assert_eq!(externalize_messages(dir.path(), &mut messages).unwrap(), 0);
}

#[test]
fn the_retired_inline_text_syntax_becomes_an_image_attachment() {
    let dir = tempfile::tempdir().unwrap();
    let text = format!(
        "before [@image](data:image/PNG;base64,{}) after",
        base64(b"legacy")
    );
    let mut messages = vec![user(&text, Vec::new())];
    assert!(has_inline_images(&messages));

    assert_eq!(externalize_messages(dir.path(), &mut messages).unwrap(), 1);

    let NormalizedMessage::User {
        text, attachments, ..
    } = &messages[0].chat.message
    else {
        unreachable!()
    };
    assert_eq!(text, "before  after");
    let payload = attachments[0].payload.as_ref().unwrap();
    assert_eq!(payload["mimeType"], "image/png");
    let image_id = attachment_image_id(&attachments[0]).unwrap();
    assert_eq!(read(dir.path(), &image_id).unwrap().unwrap(), b"legacy");
}

#[test]
fn only_old_unreferenced_files_are_swept() {
    let dir = tempfile::tempdir().unwrap();
    let kept = store(dir.path(), b"referenced").unwrap();
    let fresh = store(dir.path(), b"just written").unwrap();
    let stale = store(dir.path(), b"orphan").unwrap();
    let subagent = store(dir.path(), b"subagent").unwrap();
    let images = dir.path().join(IMAGES_DIR);
    for old in [&kept, &stale, &subagent] {
        age(&images.join(old), PAST_GRACE);
    }
    let subagent_history = format!(r#"{{"imageId":"{subagent}"}}"#);

    let removed = remove_unreferenced(dir.path(), &HashSet::from([kept.clone()]), |candidates| {
        let mut found = HashSet::new();
        ids_mentioned_in(subagent_history.as_bytes(), candidates, &mut found);
        Ok(found)
    })
    .unwrap();

    assert_eq!(removed, 1);
    assert!(read(dir.path(), &kept).unwrap().is_some());
    assert!(read(dir.path(), &fresh).unwrap().is_some());
    assert!(read(dir.path(), &subagent).unwrap().is_some());
    assert!(read(dir.path(), &stale).unwrap().is_none());
}

#[test]
fn other_histories_are_not_read_when_chat_references_every_old_file() {
    let dir = tempfile::tempdir().unwrap();
    let kept = store(dir.path(), b"referenced").unwrap();
    age(&dir.path().join(IMAGES_DIR).join(&kept), PAST_GRACE);

    let removed = remove_unreferenced(dir.path(), &HashSet::from([kept]), |_| {
        panic!("Subagent histories must not be scanned")
    })
    .unwrap();

    assert_eq!(removed, 0);
}

#[test]
fn storing_again_protects_an_old_file_from_the_sweep() {
    let dir = tempfile::tempdir().unwrap();
    let image_id = store(dir.path(), b"reused").unwrap();
    age(&dir.path().join(IMAGES_DIR).join(&image_id), PAST_GRACE);

    store(dir.path(), b"reused").unwrap();

    assert_eq!(
        remove_unreferenced(dir.path(), &HashSet::new(), |_| Ok(HashSet::new())).unwrap(),
        0
    );
}

#[test]
fn a_stored_image_records_the_size_it_displays_at() {
    let dir = tempfile::tempdir().unwrap();
    let png = crate::media::tests::encoded_png(4, 3);
    let mut messages = vec![
        user("look", vec![image_attachment(&png)]),
        agent_image(&png),
    ];

    externalize_messages(dir.path(), &mut messages).unwrap();

    let NormalizedMessage::User { attachments, .. } = &messages[0].chat.message else {
        unreachable!()
    };
    assert_eq!(attachment_dimensions(&attachments[0]), Some((4, 3)));
    let NormalizedMessage::AgentMessage { parts, .. } = &messages[1].chat.message else {
        unreachable!()
    };
    assert!(matches!(
        parts[0],
        AgentMessagePart::Image {
            width: Some(4),
            height: Some(3),
            ..
        }
    ));
    assert!(!has_references_without_dimensions(&messages));
}

#[test]
fn a_reference_whose_file_has_no_readable_header_is_left_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let mut messages = vec![user("look", vec![image_attachment(b"not an image")])];
    externalize_messages(dir.path(), &mut messages).unwrap();
    assert!(has_references_without_dimensions(&messages));

    assert_eq!(add_missing_dimensions(dir.path(), &mut messages), 0);
}
