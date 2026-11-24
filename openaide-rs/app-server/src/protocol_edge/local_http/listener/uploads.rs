use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use openaide_app_server_protocol::ids::ClientInstanceId;
use serde_json::json;

use super::http::write_http_response;
use super::{LocalHttpProbeListenerError, LocalHttpRequest};
use crate::protocol_edge::local_http::file_upload::{
    temporary_upload, AppendChunkOutcome, ChunkUploadError, ChunkUploadRequest, PendingUpload,
    UploadKind, MAX_UPLOAD_CHUNK_BYTES,
};
use crate::protocol_edge::local_http::{LocalHttpAppHandler, LocalHttpResponse};

struct CompletedUpload {
    temporary: PendingUpload,
    task_id: String,
    file_name: String,
}

/// One upload request's lifecycle: chunk sessions log their first and final request only,
/// so a healthy 64 KiB chunk stream stays quiet while a cut-off upload still shows its start.
struct UploadLog {
    mode: &'static str,
    kind: UploadKind,
    started: Instant,
}

impl UploadLog {
    fn start(
        mode: &'static str,
        kind: UploadKind,
        request: &LocalHttpRequest,
        quiet: bool,
    ) -> Self {
        if !quiet {
            crate::logging::info(
                "attachment_upload_started",
                json!({
                    "mode": mode,
                    "kind": kind.name(),
                    "task_id": request.task_id,
                    "content_length": request.content_length,
                    "declared_size": request.upload_size,
                }),
            );
        }
        Self {
            mode,
            kind,
            started: Instant::now(),
        }
    }

    fn finish(&self, outcome: &str, status: u16, error_kind: Option<&str>) {
        let fields = json!({
            "mode": self.mode,
            "kind": self.kind.name(),
            "outcome": outcome,
            "status": status,
            "duration_ms": self.started.elapsed().as_millis() as u64,
            "error_kind": error_kind,
        });
        if outcome == "success" {
            crate::logging::info("attachment_upload_completed", fields);
        } else {
            crate::logging::warn("attachment_upload_completed", fields);
        }
    }
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
    let kind = UploadKind::from_header(request.upload_kind.as_deref());
    let log = UploadLog::start("single", kind, &request, false);
    let client_instance_id = match authorize(handler, &request) {
        Ok(client_instance_id) => client_instance_id,
        Err(response) => {
            log.finish("rejected", response.status, Some("unauthorized"));
            return write_http_response(stream, &response);
        }
    };
    let (Some(task_id), Some(file_name)) =
        (request.task_id.as_deref(), request.file_name.as_deref())
    else {
        log.finish("rejected", 400, Some("missing_metadata"));
        return write_http_response(stream, &empty_response(400));
    };
    // The fast path retains one request per file while streaming directly to disk.
    stream.set_read_timeout(Some(Duration::from_secs(60)))?;
    let received = temporary_upload(task_id, file_name)
        .map_err(LocalHttpProbeListenerError::from)
        .and_then(|mut temporary| {
            stream_request_body(stream, &request, &mut temporary).map(|()| temporary)
        });
    let temporary = match received {
        Ok(temporary) => temporary,
        Err(error) => {
            log.finish("failure", 0, Some("transport"));
            return Err(error);
        }
    };
    register_upload(
        stream,
        handler,
        &client_instance_id,
        CompletedUpload {
            temporary,
            task_id: task_id.to_string(),
            file_name: file_name.to_string(),
        },
        kind,
        &log,
    )
}

fn handle_chunk_upload(
    stream: &mut TcpStream,
    handler: &LocalHttpAppHandler,
    request: LocalHttpRequest,
) -> Result<(), LocalHttpProbeListenerError> {
    let kind = UploadKind::from_header(request.upload_kind.as_deref());
    let client_instance_id = match authorize(handler, &request) {
        Ok(client_instance_id) => client_instance_id,
        Err(response) => {
            UploadLog::start("chunked", kind, &request, false).finish(
                "rejected",
                response.status,
                Some("unauthorized"),
            );
            return write_http_response(stream, &response);
        }
    };
    let Some(upload_id) = request.upload_id.as_deref() else {
        return write_http_response(stream, &empty_response(400));
    };
    if request.upload_cancel {
        handler.cancel_upload_chunk(&client_instance_id, upload_id);
        crate::logging::info(
            "attachment_upload_cancelled",
            json!({ "mode": "chunked", "kind": kind.name() }),
        );
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
    let log = UploadLog::start("chunked", kind, &request, offset != 0);
    if request.content_length > MAX_UPLOAD_CHUNK_BYTES {
        log.finish("rejected", 413, Some("chunk_too_large"));
        return write_http_response(
            stream,
            &chunk_error_response(ChunkUploadError::ChunkTooLarge {
                max: MAX_UPLOAD_CHUNK_BYTES,
            }),
        );
    }

    stream.set_read_timeout(Some(Duration::from_secs(60)))?;
    let bytes = match read_request_body(stream, &request) {
        Ok(bytes) => bytes,
        Err(error) => {
            log.finish("failure", 0, Some("transport"));
            return Err(error);
        }
    };
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
            kind,
            &log,
        ),
        Err(error) => {
            let response = chunk_error_response(error);
            log.finish(
                "rejected",
                response.status,
                Some(chunk_error_kind(&response)),
            );
            write_http_response(stream, &response)
        }
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
    kind: UploadKind,
    log: &UploadLog,
) -> Result<(), LocalHttpProbeListenerError> {
    let (_file, path) = upload.temporary.keep()?;
    let response = handler.register_uploaded_file(
        client_instance_id,
        upload.task_id,
        path.to_string_lossy().into_owned(),
        upload.file_name,
        kind,
    );
    if response.status == 200 {
        log.finish("success", 200, None);
    } else {
        cleanup_failed_upload(&path);
        log.finish("rejected", response.status, Some("registration_failed"));
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

/// A fixed class for the log; the response body may carry a free-form message.
fn chunk_error_kind(response: &LocalHttpResponse) -> &'static str {
    match response.status {
        413 => "chunk_too_large",
        404 => "missing_session",
        409 => "offset_or_metadata_mismatch",
        500 => "storage",
        _ => "invalid_chunk",
    }
}

fn empty_response(status: u16) -> LocalHttpResponse {
    LocalHttpResponse {
        status,
        body: String::new(),
    }
}
