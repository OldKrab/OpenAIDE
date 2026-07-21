use std::fs;

use openaide_app_server_protocol::ids::ClientInstanceId;

use super::{AppendChunkOutcome, ChunkUploadError, ChunkUploadRegistry, ChunkUploadRequest};

#[test]
fn assembles_ordered_chunks_and_completes_only_at_the_declared_size() {
    let registry = ChunkUploadRegistry::default();
    let client = ClientInstanceId::from("client-1");

    let first = registry
        .append(ChunkUploadRequest {
            client_instance_id: &client,
            upload_id: "upload-1",
            task_id: "task-1",
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
            task_id: "task-1",
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
    assert_eq!(task_id, "task-1");
    assert_eq!(file_name, "report.txt");
    assert_eq!(fs::read(temporary.path()).unwrap(), b"hello world");
}

#[test]
fn rejects_out_of_order_chunks_without_corrupting_the_session() {
    let registry = ChunkUploadRegistry::default();
    let client = ClientInstanceId::from("client-1");
    let request = |offset, bytes: &'static [u8]| ChunkUploadRequest {
        client_instance_id: &client,
        upload_id: "upload-1",
        task_id: "task-1",
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
    registry
        .append(ChunkUploadRequest {
            client_instance_id: &client,
            upload_id: "upload-1",
            task_id: "task-1",
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
            task_id: "task-1",
            file_name: "report.txt",
            total_size: 6,
            offset: 3,
            bytes: b"def",
        }),
        Err(ChunkUploadError::MissingSession)
    ));
}
