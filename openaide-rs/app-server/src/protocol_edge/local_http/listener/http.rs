use std::io::{Read, Write};
use std::net::TcpStream;

use crate::protocol_edge::local_http::LocalHttpResponse;

use super::LocalHttpProbeListenerError;

const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 10 * 1024 * 1024;

pub(super) struct HttpRequest {
    pub method: String,
    pub authorization: Option<String>,
    pub connection_id: Option<String>,
    pub session_id: Option<String>,
    pub after_sequence: Option<u64>,
    pub accepts_event_stream: bool,
    pub body: String,
}

pub(super) fn read_http_request(
    stream: &mut TcpStream,
) -> Result<HttpRequest, LocalHttpProbeListenerError> {
    let mut bytes = Vec::new();
    let header_end = read_headers(stream, &mut bytes)?;
    let headers = std::str::from_utf8(&bytes[..header_end])
        .map_err(|_| LocalHttpProbeListenerError::MalformedRequest("headers are not UTF-8"))?
        .to_string();
    let method = request_method(&headers)?;
    let authorization = header_value(&headers, "authorization").map(str::to_string);
    let connection_id = header_value(&headers, "x-openaide-connection-id").map(str::to_string);
    let session_id = header_value(&headers, "x-openaide-session-id").map(str::to_string);
    let after_sequence =
        header_value(&headers, "x-openaide-after").and_then(|value| value.parse::<u64>().ok());
    let accepts_event_stream = header_value(&headers, "accept").is_some_and(|value| {
        value
            .split(',')
            .any(|item| item.trim() == "text/event-stream")
    });
    let content_length = content_length(&headers, &method)?;
    if content_length > MAX_BODY_BYTES {
        return Err(LocalHttpProbeListenerError::MalformedRequest(
            "body is too large",
        ));
    }
    let body_start = header_end + 4;
    read_body(
        stream,
        &mut bytes,
        body_start,
        content_length,
        &method,
        connection_id.as_deref(),
    )?;
    let body_end = body_start + content_length;
    let body = std::str::from_utf8(&bytes[body_start..body_end])
        .map_err(|_| LocalHttpProbeListenerError::MalformedRequest("body is not UTF-8"))?
        .to_string();
    Ok(HttpRequest {
        method,
        authorization,
        connection_id,
        session_id,
        after_sequence,
        accepts_event_stream,
        body,
    })
}

pub(super) fn write_event_stream_headers(
    stream: &mut TcpStream,
) -> Result<(), LocalHttpProbeListenerError> {
    stream.write_all(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nAccess-Control-Allow-Origin: *\r\nConnection: keep-alive\r\n\r\n",
    )?;
    stream.flush()?;
    Ok(())
}

pub(super) fn write_event_stream_data(
    stream: &mut TcpStream,
    data: &str,
) -> Result<(), LocalHttpProbeListenerError> {
    write!(stream, "data: {data}\n\n")?;
    stream.flush()?;
    Ok(())
}

pub(super) fn write_event_stream_heartbeat(
    stream: &mut TcpStream,
) -> Result<(), LocalHttpProbeListenerError> {
    stream.write_all(b": heartbeat\n\n")?;
    stream.flush()?;
    Ok(())
}

pub(super) fn write_http_response(
    stream: &mut TcpStream,
    response: &LocalHttpResponse,
) -> Result<(), LocalHttpProbeListenerError> {
    let content_type = if response.body.is_empty() {
        String::new()
    } else {
        "Content-Type: application/json\r\n".to_string()
    };
    let wire = format!(
        "HTTP/1.1 {} {}\r\n{}Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Authorization, Content-Type, X-OpenAIDE-Connection-Id, X-OpenAIDE-Session-Id, X-OpenAIDE-After\r\nAccess-Control-Max-Age: 600\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response.status,
        reason_phrase(response.status),
        content_type,
        response.body.len(),
        response.body
    );
    stream.write_all(wire.as_bytes())?;
    Ok(())
}

fn read_headers(
    stream: &mut TcpStream,
    bytes: &mut Vec<u8>,
) -> Result<usize, LocalHttpProbeListenerError> {
    loop {
        let mut chunk = [0_u8; 512];
        let read = stream.read(&mut chunk).map_err(|error| {
            LocalHttpProbeListenerError::Io(error).with_request_context(
                "read_headers",
                None,
                None,
                None,
            )
        })?;
        if read == 0 {
            return Err(LocalHttpProbeListenerError::MalformedRequest(
                "connection closed before headers completed",
            ));
        }
        bytes.extend_from_slice(&chunk[..read]);
        if let Some(index) = header_end_index(bytes) {
            return Ok(index);
        }
        if bytes.len() > MAX_HEADER_BYTES {
            return Err(LocalHttpProbeListenerError::MalformedRequest(
                "headers are too large",
            ));
        }
    }
}

fn read_body(
    stream: &mut TcpStream,
    bytes: &mut Vec<u8>,
    body_start: usize,
    content_length: usize,
    method: &str,
    connection_id: Option<&str>,
) -> Result<(), LocalHttpProbeListenerError> {
    while bytes.len() - body_start < content_length {
        let mut chunk = [0_u8; 512];
        let read = stream.read(&mut chunk).map_err(|error| {
            LocalHttpProbeListenerError::Io(error).with_request_context(
                "read_body",
                Some(method),
                connection_id,
                None,
            )
        })?;
        if read == 0 {
            return Err(LocalHttpProbeListenerError::MalformedRequest(
                "connection closed before body completed",
            ));
        }
        bytes.extend_from_slice(&chunk[..read]);
    }
    Ok(())
}

fn request_method(headers: &str) -> Result<String, LocalHttpProbeListenerError> {
    let request_line =
        headers
            .lines()
            .next()
            .ok_or(LocalHttpProbeListenerError::MalformedRequest(
                "missing request line",
            ))?;
    let method = request_line.split_whitespace().next().ok_or(
        LocalHttpProbeListenerError::MalformedRequest("missing method"),
    )?;
    Ok(method.to_string())
}

fn content_length(headers: &str, method: &str) -> Result<usize, LocalHttpProbeListenerError> {
    let Some(value) = header_value(headers, "content-length") else {
        // Browser CORS preflights have no body and are not required to carry a
        // Content-Length header. Protocol POST requests remain strictly framed.
        return if method == "OPTIONS" || method == "GET" {
            Ok(0)
        } else {
            Err(LocalHttpProbeListenerError::MalformedRequest(
                "missing Content-Length",
            ))
        };
    };
    value
        .parse::<usize>()
        .map_err(|_| LocalHttpProbeListenerError::MalformedRequest("invalid Content-Length"))
}

fn header_value<'a>(headers: &'a str, name: &str) -> Option<&'a str> {
    headers.lines().skip(1).find_map(|line| {
        let (header_name, value) = line.split_once(':')?;
        header_name
            .eq_ignore_ascii_case(name)
            .then_some(value.trim())
    })
}

fn header_end_index(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        _ => "Status",
    }
}
