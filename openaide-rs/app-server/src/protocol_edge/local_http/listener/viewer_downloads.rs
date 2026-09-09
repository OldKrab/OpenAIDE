use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Instant;

use super::{
    query_value, write_http_response, LocalHttpAppHandler, LocalHttpProbeListenerError,
    LocalHttpRequest, LocalHttpResponse,
};

/// Viewer downloads retain path identity (including symlink aliases), not the preview snapshot.
pub(super) fn download(
    stream: &mut TcpStream,
    handler: &LocalHttpAppHandler,
    request: &LocalHttpRequest,
    handle: &str,
) -> Result<(), LocalHttpProbeListenerError> {
    let started = Instant::now();
    let operation_id = query_value(&request.target, "operationId")
        .and_then(|value| uuid::Uuid::parse_str(&value).ok())
        .unwrap_or_else(uuid::Uuid::new_v4)
        .to_string();
    let phase = if query_value(&request.target, "check").as_deref() == Some("1") {
        "check"
    } else {
        "transfer"
    };
    crate::logging::info(
        "file_viewer_download_started",
        serde_json::json!({
            "operation_id": operation_id, "attempt": 1, "phase": phase,
        }),
    );
    let result = serve(stream, handler, request, handle);
    crate::logging::info(
        "file_viewer_download_completed",
        serde_json::json!({
            "operation_id": operation_id, "attempt": 1, "phase": phase,
            "duration_ms": started.elapsed().as_millis(),
            "outcome": if matches!(result, Ok(200 | 204)) { "success" } else { "failure" },
            "http_status": result.as_ref().ok(),
            "error_kind": match &result {
                Ok(200 | 204) => None,
                Ok(400) => Some("invalid_target"),
                Ok(401 | 403) => Some("permission_denied"),
                Ok(404) => Some("not_found"),
                Ok(409) => Some("client_unavailable"),
                Ok(_) => Some("unreadable"),
                Err(_) => Some("transfer_io"),
            },
        }),
    );
    result.map(|_| ())
}

fn serve(
    stream: &mut TcpStream,
    handler: &LocalHttpAppHandler,
    request: &LocalHttpRequest,
    handle: &str,
) -> Result<u16, LocalHttpProbeListenerError> {
    let path = match handler.resolve_file_viewer_download(
        request.authorization.as_deref(),
        query_value(&request.target, "clientInstanceId").as_deref(),
        handle,
    ) {
        Ok(path) => path,
        Err(response) => {
            write_http_response(stream, &response)?;
            return Ok(response.status);
        }
    };
    let (file, size) = match open_regular_file(&path) {
        Ok(opened) => opened,
        Err(status) => {
            write_http_response(
                stream,
                &LocalHttpResponse {
                    status,
                    body: String::new(),
                },
            )?;
            return Ok(status);
        }
    };
    // Check readability without transferring bytes. GET reopens and validates again because
    // the Agent may replace/delete the file before the browser starts its managed transfer.
    if query_value(&request.target, "check").as_deref() == Some("1") {
        write_http_response(
            stream,
            &LocalHttpResponse {
                status: 204,
                body: String::new(),
            },
        )?;
        return Ok(204);
    }
    let label = path.file_name().unwrap_or_default().to_string_lossy();
    // RFC 8187 retains Unicode and punctuation; quoted ASCII is a legacy fallback only.
    let fallback: String = label
        .chars()
        .map(|c| {
            if c.is_ascii_graphic() && !matches!(c, '"' | '\\' | '%') || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let encoded: String = label
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect();
    write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=\"{fallback}\"; filename*=UTF-8''{encoded}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nContent-Length: {size}\r\nConnection: close\r\n\r\n")?;
    // Bound the transfer to its initial length even when an Agent is appending to the file.
    let copied = std::io::copy(&mut file.take(size), stream)?;
    if copied != size {
        return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof).into());
    }
    Ok(200)
}

fn open_regular_file(path: &std::path::Path) -> Result<(std::fs::File, u64), u16> {
    let metadata = std::fs::metadata(path).map_err(io_status)?;
    if !metadata.is_file() {
        return Err(400);
    }
    let file = std::fs::File::open(path).map_err(io_status)?;
    let metadata = file.metadata().map_err(io_status)?;
    if !metadata.is_file() {
        return Err(400);
    }
    Ok((file, metadata.len()))
}

fn io_status(error: std::io::Error) -> u16 {
    match error.kind() {
        std::io::ErrorKind::NotFound => 404,
        std::io::ErrorKind::PermissionDenied => 403,
        _ => 500,
    }
}
