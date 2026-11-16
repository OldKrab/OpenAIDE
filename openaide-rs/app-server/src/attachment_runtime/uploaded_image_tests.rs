use base64::Engine;
use openaide_app_server_protocol::attachment::AttachmentResourceId;
use openaide_app_server_protocol::ids::TaskId;

use crate::attachment_runtime::{AttachmentRuntime, AttachmentRuntimeError};

const PNG_BYTES: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR rest of a photo";

fn upload(dir: &tempfile::TempDir, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn send_delivers_uploaded_image_bytes_typed_by_their_content() {
    let dir = tempfile::tempdir().unwrap();
    // The name claims JPEG; the bytes decide the media type.
    let path = upload(&dir, "photo.jpg", PNG_BYTES);
    let runtime = AttachmentRuntime::new();
    let task_id = TaskId::from("task-1");

    let created = runtime
        .create_uploaded_image(&task_id, &path, "photo.jpg".to_string())
        .unwrap();
    let resolved = runtime
        .resolve_for_send(&task_id, &[created.handle_id])
        .unwrap();

    assert_eq!(created.label, "photo.jpg");
    for attachment in [
        &resolved.chat_attachments()[0],
        &resolved.agent_attachments()[0],
    ] {
        let payload = attachment.payload.as_ref().unwrap();
        assert_eq!(attachment.kind, "image");
        assert_eq!(payload["mimeType"], "image/png");
        assert_eq!(payload["sizeBytes"], PNG_BYTES.len());
        assert_eq!(
            payload["data"],
            base64::engine::general_purpose::STANDARD.encode(PNG_BYTES)
        );
    }
}

#[test]
fn uploaded_image_has_no_size_limit_of_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let mut bytes = PNG_BYTES.to_vec();
    // Larger than both inline Image limits (5 MiB each, 8 MiB per message).
    bytes.resize(9 * 1024 * 1024, 0);
    let path = upload(&dir, "large.png", &bytes);
    let runtime = AttachmentRuntime::new();
    let task_id = TaskId::from("task-1");

    let created = runtime
        .create_uploaded_image(&task_id, &path, "large.png".to_string())
        .unwrap();

    let resolved = runtime
        .resolve_for_send(&task_id, &[created.handle_id])
        .unwrap();
    assert_eq!(
        resolved.agent_attachments()[0].payload.as_ref().unwrap()["sizeBytes"],
        bytes.len()
    );
}

#[test]
fn rejects_uploads_that_are_not_a_supported_image() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = AttachmentRuntime::new();
    let task_id = TaskId::from("task-1");
    let text = upload(&dir, "notes.png", b"plain text, not an image");
    let empty = upload(&dir, "empty.png", b"");

    assert_eq!(
        runtime
            .create_uploaded_image(&task_id, &text, "notes.png".to_string())
            .unwrap_err(),
        AttachmentRuntimeError::UnsupportedImage
    );
    assert_eq!(
        runtime
            .create_uploaded_image(&task_id, &empty, "empty.png".to_string())
            .unwrap_err(),
        AttachmentRuntimeError::InvalidImage
    );
    // A rejected upload stays with the caller, which removes it with the failed request.
    assert!(text.exists());
}

#[test]
fn releasing_or_sending_an_uploaded_image_removes_its_file() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = AttachmentRuntime::new();
    let task_id = TaskId::from("task-1");
    let released_path = upload(&dir, "released.png", PNG_BYTES);
    let sent_path = upload(&dir, "sent.png", PNG_BYTES);
    let released = runtime
        .create_uploaded_image(&task_id, &released_path, "released.png".to_string())
        .unwrap();
    let sent = runtime
        .create_uploaded_image(&task_id, &sent_path, "sent.png".to_string())
        .unwrap();

    runtime.release_resources(
        &task_id,
        &[AttachmentResourceId::Handle {
            id: released.handle_id,
        }],
    );
    let reservation = runtime
        .reserve_for_send(&task_id, std::slice::from_ref(&sent.handle_id))
        .unwrap();
    // A reserved Image keeps its file until Send commits.
    assert!(sent_path.exists());
    let committed = reservation.commit();

    assert!(!released_path.exists());
    assert!(!sent_path.exists());
    assert_eq!(committed.agent_attachments()[0].kind, "image");
}

#[test]
fn a_vanished_upload_file_reports_a_recoverable_unknown_handle() {
    let dir = tempfile::tempdir().unwrap();
    let path = upload(&dir, "photo.png", PNG_BYTES);
    let runtime = AttachmentRuntime::new();
    let task_id = TaskId::from("task-1");
    let created = runtime
        .create_uploaded_image(&task_id, &path, "photo.png".to_string())
        .unwrap();

    std::fs::remove_file(&path).unwrap();

    assert_eq!(
        runtime
            .resolve_for_send(&task_id, &[created.handle_id])
            .unwrap_err(),
        AttachmentRuntimeError::UnknownHandle
    );
}
