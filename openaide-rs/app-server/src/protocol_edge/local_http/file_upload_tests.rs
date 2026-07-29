use std::fs;

use openaide_app_server_protocol::ids::ClientInstanceId;

use super::{
    safe_upload_file_name, temporary_upload, AppendChunkOutcome, ChunkUploadError,
    ChunkUploadRegistry, ChunkUploadRequest,
};

#[test]
fn upload_uses_the_original_basename_inside_the_openaide_temp_directory() {
    let task_id = test_task_id("original-name");
    let upload = temporary_upload(&task_id, "quarterly report.jsonl").unwrap();
    let path = upload.path();

    assert_eq!(
        path.file_name().and_then(|name| name.to_str()),
        Some("quarterly report.jsonl")
    );
    assert!(path
        .ancestors()
        .any(|ancestor| ancestor.ends_with("openaide/uploads")));
}

#[test]
fn same_name_uploads_add_a_numeric_suffix_inside_the_task_directory() {
    let task_id = test_task_id("collision");
    let first = temporary_upload(&task_id, "report.jsonl").unwrap();
    let second = temporary_upload(&task_id, "report.jsonl").unwrap();

    assert_eq!(first.path().parent(), second.path().parent());
    assert_eq!(
        first.path().file_name().and_then(|name| name.to_str()),
        Some("report.jsonl")
    );
    assert_eq!(
        second.path().file_name().and_then(|name| name.to_str()),
        Some("report-2.jsonl")
    );
}

#[test]
fn abandoned_upload_removes_its_partial_file() {
    let task_id = test_task_id("abandoned");
    let path = {
        let upload = temporary_upload(&task_id, "partial.bin").unwrap();
        upload.path().to_path_buf()
    };

    assert!(!path.exists());
}

#[test]
fn upload_name_is_safe_on_every_supported_platform() {
    assert_eq!(
        safe_upload_file_name(r"C:\exports\report?.jsonl"),
        "report_.jsonl"
    );
    assert_eq!(safe_upload_file_name("CON"), "_CON");
    assert_eq!(safe_upload_file_name("../"), "Attached file");

    let long_name = format!("{}.jsonl", "a".repeat(170));
    let shortened = safe_upload_file_name(&long_name);
    assert_eq!(shortened.chars().count(), 160);
    assert!(shortened.ends_with(".jsonl"));
}

fn test_task_id(scope: &str) -> String {
    format!("task-{scope}-{}", uuid::Uuid::new_v4())
}

#[test]
fn assembles_ordered_chunks_and_completes_only_at_the_declared_size() {
    let registry = ChunkUploadRegistry::default();
    let client = ClientInstanceId::from("client-1");
    let expected_task_id = test_task_id("assemble");

    let first = registry
        .append(ChunkUploadRequest {
            client_instance_id: &client,
            upload_id: "upload-1",
            task_id: &expected_task_id,
            file_name: "report.txt",
            total_size: 11,
            offset: 0,
            bytes: b"hello ",
        })
        .unwrap();
    assert!(matches!(first, AppendChunkOutcome::Partial { received: 6 }));

    let second = registry
        .append(ChunkUploadRequest {
            client_instance_id: &client,
            upload_id: "upload-1",
            task_id: &expected_task_id,
            file_name: "report.txt",
            total_size: 11,
            offset: 6,
            bytes: b"world",
        })
        .unwrap();
    let AppendChunkOutcome::Complete {
        temporary,
        task_id,
        file_name,
    } = second
    else {
        panic!("the final chunk must complete the upload");
    };
    assert_eq!(task_id, expected_task_id);
    assert_eq!(file_name, "report.txt");
    assert_eq!(
        temporary.path().file_name().and_then(|name| name.to_str()),
        Some("report.txt")
    );
    assert_eq!(fs::read(temporary.path()).unwrap(), b"hello world");
}

#[test]
fn rejects_out_of_order_chunks_without_corrupting_the_session() {
    let registry = ChunkUploadRegistry::default();
    let client = ClientInstanceId::from("client-1");
    let task_id = test_task_id("out-of-order");
    let request = |offset, bytes: &'static [u8]| ChunkUploadRequest {
        client_instance_id: &client,
        upload_id: "upload-1",
        task_id: &task_id,
        file_name: "report.txt",
        total_size: 6,
        offset,
        bytes,
    };

    registry.append(request(0, b"abc")).unwrap();
    assert!(matches!(
        registry.append(request(2, b"bad")),
        Err(ChunkUploadError::OffsetMismatch { expected: 3 })
    ));
    assert!(matches!(
        registry.append(request(3, b"def")).unwrap(),
        AppendChunkOutcome::Complete { .. }
    ));
}

#[test]
fn cancellation_discards_the_partial_session() {
    let registry = ChunkUploadRegistry::default();
    let client = ClientInstanceId::from("client-1");
    let task_id = test_task_id("cancellation");
    registry
        .append(ChunkUploadRequest {
            client_instance_id: &client,
            upload_id: "upload-1",
            task_id: &task_id,
            file_name: "report.txt",
            total_size: 6,
            offset: 0,
            bytes: b"abc",
        })
        .unwrap();

    assert!(registry.cancel(&client, "upload-1"));
    assert!(matches!(
        registry.append(ChunkUploadRequest {
            client_instance_id: &client,
            upload_id: "upload-1",
            task_id: &task_id,
            file_name: "report.txt",
            total_size: 6,
            offset: 3,
            bytes: b"def",
        }),
        Err(ChunkUploadError::MissingSession)
    ));
}
