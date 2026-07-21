use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use openaide_app_server_protocol::ids::ClientInstanceId;
use serde_json::json;
use tempfile::NamedTempFile;

use super::http::write_http_response;
use super::{LocalHttpProbeListenerError, LocalHttpRequest};
use crate::protocol_edge::local_http::file_upload::{
    temporary_upload, AppendChunkOutcome, ChunkUploadError, ChunkUploadRequest,
    MAX_UPLOAD_CHUNK_BYTES,
};
use crate::protocol_edge::local_http::{LocalHttpAppHandler, LocalHttpResponse};

struct CompletedUpload {
    temporary: NamedTempFile,
    task_id: String,
    file_name: String,
}

pub(super) fn handle_file_upload(
    stream: &mut TcpStream,
    handler: &LocalHttpAppHandler,
    request: LocalHttpRequest,
) -> Result<(), LocalHttpProbeListenerError> {
    if request
        .target
        .split('?')
        .next()
        .is_some_and(|path| path.ends_with("/upload/chunk"))
    {
        return handle_chunk_upload(stream, handler, request);
    }
    handle_single_upload(stream, handler, request)
}

fn handle_single_upload(
    stream: &mut TcpStream,
    handler: &LocalHttpAppHandler,
    request: LocalHttpRequest,
) -> Result<(), LocalHttpProbeListenerError> {
    let client_instance_id = match authorize(handler, &request) {
        Ok(client_instance_id) => client_instance_id,
        Err(response) => return write_http_response(stream, &response),
    };
    let (Some(task_id), Some(file_name)) =
        (request.task_id.as_deref(), request.file_name.as_deref())
    else {
        return write_http_response(stream, &empty_response(400));
    };
    // The fast path retains one request per file while streaming directly to disk.
    stream.set_read_timeout(Some(Duration::from_secs(60)))?;
    let mut temporary = temporary_upload(file_name)?;
    stream_request_body(stream, &request, &mut temporary)?;
    register_upload(
        stream,
        handler,
        &client_instance_id,
        CompletedUpload {
            temporary,
            task_id: task_id.to_string(),
            file_name: file_name.to_string(),
        },
    )
}

fn handle_chunk_upload(
    stream: &mut TcpStream,
    handler: &LocalHttpAppHandler,
    request: LocalHttpRequest,
) -> Result<(), LocalHttpProbeListenerError> {
    let client_instance_id = match authorize(handler, &request) {
        Ok(client_instance_id) => client_instance_id,
        Err(response) => return write_http_response(stream, &response),
    };
    let Some(upload_id) = request.upload_id.as_deref() else {
        return write_http_response(stream, &empty_response(400));
    };
    if request.upload_cancel {
        handler.cancel_upload_chunk(&client_instance_id, upload_id);
        return write_http_response(stream, &empty_response(204));
    }
    let (Some(task_id), Some(file_name), Some(offset), Some(total_size)) = (
        request.task_id.as_deref(),
        request.file_name.as_deref(),
        request.upload_offset,
        request.upload_size,
    ) else {
        return write_http_response(stream, &empty_response(400));
    };
    if request.content_length > MAX_UPLOAD_CHUNK_BYTES {
        return write_http_response(
            stream,
            &chunk_error_response(ChunkUploadError::ChunkTooLarge {
                max: MAX_UPLOAD_CHUNK_BYTES,
            }),
        );
    }

    stream.set_read_timeout(Some(Duration::from_secs(60)))?;
    let bytes = read_request_body(stream, &request)?;
    match handler.append_upload_chunk(ChunkUploadRequest {
        client_instance_id: &client_instance_id,
        upload_id,
        task_id,
        file_name,
        total_size,
        offset,
        bytes: &bytes,
    }) {
        Ok(AppendChunkOutcome::Partial { received }) => write_http_response(
            stream,
            &LocalHttpResponse {
                status: 202,
                body: json!({ "received": received }).to_string(),
            },
        ),
        Ok(AppendChunkOutcome::Complete {
            temporary,
            task_id,
            file_name,
        }) => register_upload(
            stream,
            handler,
            &client_instance_id,
            CompletedUpload {
                temporary,
                task_id,
                file_name,
            },
        ),
        Err(error) => write_http_response(stream, &chunk_error_response(error)),
    }
}

fn authorize(
    handler: &LocalHttpAppHandler,
    request: &LocalHttpRequest,
) -> Result<ClientInstanceId, LocalHttpResponse> {
    handler.authorize_upload(
        request.authorization.as_deref(),
        request.client_instance_id.as_deref(),
    )
}

fn read_request_body(
    stream: &mut TcpStream,
    request: &LocalHttpRequest,
) -> Result<Vec<u8>, LocalHttpProbeListenerError> {
    let initial = request.initial_body.len().min(request.content_length);
    let mut bytes = Vec::with_capacity(request.content_length);
    bytes.extend_from_slice(&request.initial_body[..initial]);
    let mut chunk = [0_u8; 64 * 1024];
    while bytes.len() < request.content_length {
        let wanted = chunk.len().min(request.content_length - bytes.len());
        let read = stream.read(&mut chunk[..wanted])?;
        if read == 0 {
            return Err(LocalHttpProbeListenerError::MalformedRequest(
                "connection closed before upload completed",
            ));
        }
        bytes.extend_from_slice(&chunk[..read]);
    }
    Ok(bytes)
}

fn stream_request_body(
    stream: &mut TcpStream,
    request: &LocalHttpRequest,
    destination: &mut impl Write,
) -> Result<(), LocalHttpProbeListenerError> {
    let initial = request.initial_body.len().min(request.content_length);
    destination.write_all(&request.initial_body[..initial])?;
    let mut received = initial;
    let mut chunk = [0_u8; 64 * 1024];
    while received < request.content_length {
        let wanted = chunk.len().min(request.content_length - received);
        let read = stream.read(&mut chunk[..wanted])?;
        if read == 0 {
            return Err(LocalHttpProbeListenerError::MalformedRequest(
                "connection closed before upload completed",
            ));
        }
        destination.write_all(&chunk[..read])?;
        received += read;
    }
    destination.flush()?;
    Ok(())
}

fn register_upload(
    stream: &mut TcpStream,
    handler: &LocalHttpAppHandler,
    client_instance_id: &ClientInstanceId,
    upload: CompletedUpload,
) -> Result<(), LocalHttpProbeListenerError> {
    let (_file, path) = upload.temporary.keep().map_err(|error| error.error)?;
    let response = handler.register_uploaded_file(
        client_instance_id,
        upload.task_id,
        path.to_string_lossy().into_owned(),
        upload.file_name,
    );
    if response.status != 200 {
        cleanup_failed_upload(&path);
    }
    write_http_response(stream, &response)
}

fn cleanup_failed_upload(path: &std::path::Path) {
    if let Err(error) = std::fs::remove_file(path) {
        crate::logging::error(
            "attachment_upload_cleanup_failed",
            json!({ "error": error.to_string() }),
        );
    }
}

fn chunk_error_response(error: ChunkUploadError) -> LocalHttpResponse {
    let status = match &error {
        ChunkUploadError::ChunkTooLarge { .. } => 413,
        ChunkUploadError::MissingSession => 404,
        ChunkUploadError::MetadataMismatch | ChunkUploadError::OffsetMismatch { .. } => 409,
        ChunkUploadError::Io(_) | ChunkUploadError::StateUnavailable => 500,
        ChunkUploadError::InvalidUploadId | ChunkUploadError::SizeExceeded => 400,
    };
    LocalHttpResponse {
        status,
        body: json!({ "error": { "message": error.to_string() } }).to_string(),
    }
}

fn empty_response(status: u16) -> LocalHttpResponse {
    LocalHttpResponse {
        status,
        body: String::new(),
    }
}
