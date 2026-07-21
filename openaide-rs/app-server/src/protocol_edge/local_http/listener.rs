use std::io::ErrorKind;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

use serde_json::{json, Value};
use thiserror::Error;

use super::{LocalHttpAppHandler, LocalHttpProbeHandler, LocalHttpResponse};

const DEFAULT_TIMEOUT: Duration = Duration::from_millis(750);

mod http;
mod uploads;

use http::{
    read_http_request, write_event_stream_data, write_event_stream_headers,
    write_event_stream_heartbeat, write_file_download, write_http_response,
};
use uploads::handle_file_upload;

pub struct LocalHttpProbeListener {
    listener: TcpListener,
    timeout: Duration,
}

impl LocalHttpProbeListener {
    pub fn bind_loopback() -> Result<Self, LocalHttpProbeListenerError> {
        Self::bind("127.0.0.1:0".parse().expect("static loopback address"))
    }

    pub fn bind(address: SocketAddr) -> Result<Self, LocalHttpProbeListenerError> {
        if !address.ip().is_loopback() {
            return Err(LocalHttpProbeListenerError::NonLoopbackBind);
        }
        let listener = TcpListener::bind(address)?;
        Ok(Self {
            listener,
            timeout: DEFAULT_TIMEOUT,
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr, LocalHttpProbeListenerError> {
        Ok(self.listener.local_addr()?)
    }

    pub fn handle_next(
        &self,
        handler: &mut LocalHttpProbeHandler,
    ) -> Result<(), LocalHttpProbeListenerError> {
        let (mut stream, peer) = self.listener.accept()?;
        if !peer.ip().is_loopback() {
            return Err(LocalHttpProbeListenerError::NonLoopbackPeer);
        }
        configure_timeouts(&stream, self.timeout)?;
        handle_stream(&mut stream, |request| {
            handler.handle(request.authorization.as_deref(), &request.body)
        })
    }

    pub fn handle_next_app(
        &self,
        handler: &mut LocalHttpAppHandler,
    ) -> Result<(), LocalHttpProbeListenerError> {
        let (mut stream, peer) = self.listener.accept()?;
        if !peer.ip().is_loopback() {
            return Err(LocalHttpProbeListenerError::NonLoopbackPeer);
        }
        configure_timeouts(&stream, self.timeout)?;
        handle_stream(&mut stream, |request| {
            handler.handle(
                request.authorization.as_deref(),
                request.connection_id.as_deref(),
                &request.body,
            )
        })
    }

    pub fn accept(&self) -> Result<TcpStream, LocalHttpProbeListenerError> {
        let (stream, peer) = self.listener.accept()?;
        if !peer.ip().is_loopback() {
            return Err(LocalHttpProbeListenerError::NonLoopbackPeer);
        }
        configure_timeouts(&stream, self.timeout)?;
        Ok(stream)
    }
}

pub(crate) struct LocalHttpRequest {
    pub method: String,
    pub target: String,
    pub authorization: Option<String>,
    pub connection_id: Option<String>,
    pub client_instance_id: Option<String>,
    pub task_id: Option<String>,
    pub file_name: Option<String>,
    pub upload_id: Option<String>,
    pub upload_offset: Option<usize>,
    pub upload_size: Option<usize>,
    pub upload_cancel: bool,
    pub session_id: Option<String>,
    pub after_sequence: Option<u64>,
    pub accepts_event_stream: bool,
    pub content_length: usize,
    pub initial_body: Vec<u8>,
    pub body: String,
}

pub fn handle_app_stream(
    stream: &mut TcpStream,
    handler: &LocalHttpAppHandler,
) -> Result<(), LocalHttpProbeListenerError> {
    handle_stream_with_routes(
        stream,
        |request| {
            handler.handle(
                request.authorization.as_deref(),
                request.connection_id.as_deref(),
                &request.body,
            )
        },
        |stream, request| handle_event_stream(stream, handler, request),
        |_stream, request| Ok(handle_session_poll(handler, request)),
        |stream, request| handle_file_upload(stream, handler, request),
        |stream, request| handle_file_download(stream, handler, request),
    )
}

pub(crate) fn handle_stream(
    stream: &mut TcpStream,
    handler: impl FnOnce(LocalHttpRequest) -> LocalHttpResponse,
) -> Result<(), LocalHttpProbeListenerError> {
    handle_stream_with_push(stream, handler, |_stream, _request| unreachable!())
}

fn handle_stream_with_push(
    stream: &mut TcpStream,
    handler: impl FnOnce(LocalHttpRequest) -> LocalHttpResponse,
    push: impl FnOnce(&mut TcpStream, LocalHttpRequest) -> Result<(), LocalHttpProbeListenerError>,
) -> Result<(), LocalHttpProbeListenerError> {
    handle_stream_with_routes(
        stream,
        handler,
        push,
        |_stream, _request| {
            Ok(LocalHttpResponse {
                status: 405,
                body: String::new(),
            })
        },
        |_stream, _request| Ok(()),
        |_stream, _request| Ok(()),
    )
}

fn handle_stream_with_routes(
    stream: &mut TcpStream,
    handler: impl FnOnce(LocalHttpRequest) -> LocalHttpResponse,
    push: impl FnOnce(&mut TcpStream, LocalHttpRequest) -> Result<(), LocalHttpProbeListenerError>,
    receive: impl FnOnce(
        &mut TcpStream,
        LocalHttpRequest,
    ) -> Result<LocalHttpResponse, LocalHttpProbeListenerError>,
    upload: impl FnOnce(&mut TcpStream, LocalHttpRequest) -> Result<(), LocalHttpProbeListenerError>,
    download: impl FnOnce(&mut TcpStream, LocalHttpRequest) -> Result<(), LocalHttpProbeListenerError>,
) -> Result<(), LocalHttpProbeListenerError> {
    let request = match read_http_request(stream) {
        Ok(request) => LocalHttpRequest {
            method: request.method,
            target: request.target,
            authorization: request.authorization,
            connection_id: request.connection_id,
            client_instance_id: request.client_instance_id,
            task_id: request.task_id,
            file_name: request.file_name,
            upload_id: request.upload_id,
            upload_offset: request.upload_offset,
            upload_size: request.upload_size,
            upload_cancel: request.upload_cancel,
            session_id: request.session_id,
            after_sequence: request.after_sequence,
            accepts_event_stream: request.accepts_event_stream,
            content_length: request.content_length,
            initial_body: request.initial_body,
            body: request.body,
        },
        Err(error) => {
            let _ = write_http_response(
                stream,
                &LocalHttpResponse {
                    status: 400,
                    body: String::new(),
                },
            );
            return Err(error);
        }
    };
    if request.method == "GET"
        && request
            .target
            .split('?')
            .next()
            .is_some_and(|path| path.ends_with("/download"))
    {
        return download(stream, request);
    }
    if request.method != "POST" {
        if request.method == "OPTIONS" {
            write_http_response(
                stream,
                &LocalHttpResponse {
                    status: 204,
                    body: String::new(),
                },
            )
            .map_err(|error| {
                error.with_request_context("write_response", Some(&request.method), None, Some(204))
            })?;
            return Ok(());
        }
        if request.method == "GET" {
            let method = request.method.clone();
            let connection_id = request.connection_id.clone();
            let response = receive(stream, request)?;
            let status = response.status;
            return write_http_response(stream, &response).map_err(|error| {
                error.with_request_context(
                    "write_response",
                    Some(&method),
                    connection_id.as_deref(),
                    Some(status),
                )
            });
        }
        write_http_response(
            stream,
            &LocalHttpResponse {
                status: 405,
                body: String::new(),
            },
        )
        .map_err(|error| {
            error.with_request_context("write_response", Some(&request.method), None, Some(405))
        })?;
        return Ok(());
    }
    let method = request.method.clone();
    let connection_id = request.connection_id.clone();
    if request
        .target
        .split('?')
        .next()
        .is_some_and(|path| path.ends_with("/upload") || path.ends_with("/upload/chunk"))
    {
        return upload(stream, request);
    }
    if request.accepts_event_stream {
        return push(stream, request);
    }
    let response = handler(request);
    let status = response.status;
    write_http_response(stream, &response).map_err(|error| {
        error.with_request_context(
            "write_response",
            Some(&method),
            connection_id.as_deref(),
            Some(status),
        )
    })
}

fn handle_file_download(
    stream: &mut TcpStream,
    handler: &LocalHttpAppHandler,
    request: LocalHttpRequest,
) -> Result<(), LocalHttpProbeListenerError> {
    let target = request.target.as_str();
    let attachment_index =
        query_value(target, "attachmentIndex").and_then(|value| value.parse().ok());
    let resolved = match handler.resolve_sent_file(
        request.authorization.as_deref(),
        query_value(target, "clientInstanceId").as_deref(),
        query_value(target, "taskId").as_deref(),
        query_value(target, "messageId").as_deref(),
        attachment_index,
    ) {
        Ok(resolved) => resolved,
        Err(response) => return write_http_response(stream, &response),
    };
    write_file_download(stream, &resolved.path, &resolved.label)
}

fn query_value(target: &str, name: &str) -> Option<String> {
    target.split_once('?')?.1.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        (percent_decode(key) == name).then(|| percent_decode(value))
    })
}

fn percent_decode(value: &str) -> String {
    let mut decoded = Vec::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
            {
                decoded.push(high * 16 + low);
                index += 3;
                continue;
            }
        } else if bytes[index] == b'+' {
            decoded.push(b' ');
            index += 1;
            continue;
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn handle_session_poll(
    handler: &LocalHttpAppHandler,
    request: LocalHttpRequest,
) -> LocalHttpResponse {
    let (Some(session_id), Some(after)) = (request.session_id.as_deref(), request.after_sequence)
    else {
        return LocalHttpResponse {
            status: 400,
            body: String::new(),
        };
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(25);
    loop {
        let response = handler.poll_session(
            request.authorization.as_deref(),
            request.connection_id.as_deref(),
            session_id,
            after,
        );
        if response.status != 204 || std::time::Instant::now() >= deadline {
            return response;
        }
        std::thread::sleep(Duration::from_millis(16));
    }
}

fn handle_event_stream(
    stream: &mut TcpStream,
    handler: &LocalHttpAppHandler,
    request: LocalHttpRequest,
) -> Result<(), LocalHttpProbeListenerError> {
    let lease = match handler.begin_event_stream(
        request.authorization.as_deref(),
        request.connection_id.as_deref(),
    ) {
        Ok(connection_id) => connection_id,
        Err(response) => return write_http_response(stream, &response),
    };
    let result = (|| {
        write_event_stream_headers(stream)?;
        let mut last_heartbeat = std::time::Instant::now();
        while handler.event_stream_is_current(&lease) {
            let messages = handler.drain_push_messages(&lease);
            if !messages.is_empty() {
                write_event_stream_data(stream, &messages)?;
                if !handler.observe_event_stream_activity(&lease) {
                    break;
                }
                last_heartbeat = std::time::Instant::now();
            } else if last_heartbeat.elapsed() >= Duration::from_secs(1) {
                write_event_stream_heartbeat(stream)?;
                if !handler.observe_event_stream_activity(&lease) {
                    break;
                }
                last_heartbeat = std::time::Instant::now();
            }
            std::thread::sleep(Duration::from_millis(16));
        }
        Ok(())
    })();
    handler.finish_event_stream(&lease);
    result
}

fn configure_timeouts(
    stream: &TcpStream,
    timeout: Duration,
) -> Result<(), LocalHttpProbeListenerError> {
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    Ok(())
}

#[derive(Debug, Error)]
pub enum LocalHttpProbeListenerError {
    #[error("LocalHttp probe listener I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("LocalHttp probe listener I/O failed during {phase}: {source}")]
    IoWithContext {
        phase: &'static str,
        method: Option<String>,
        connection_id: Option<String>,
        status: Option<u16>,
        #[source]
        source: std::io::Error,
    },
    #[error("LocalHttp probe listener must bind loopback")]
    NonLoopbackBind,
    #[error("LocalHttp probe peer must be loopback")]
    NonLoopbackPeer,
    #[error("malformed LocalHttp probe request: {0}")]
    MalformedRequest(&'static str),
}

impl LocalHttpProbeListenerError {
    pub fn with_request_context(
        self,
        phase: &'static str,
        method: Option<&str>,
        connection_id: Option<&str>,
        status: Option<u16>,
    ) -> Self {
        match self {
            Self::Io(source) => Self::IoWithContext {
                phase,
                method: method.map(str::to_string),
                connection_id: connection_id.map(str::to_string),
                status,
                source,
            },
            error => error,
        }
    }

    pub fn is_transient_io(&self) -> bool {
        matches!(
            self,
            Self::Io(error) if is_transient_socket_kind(error.kind())
        ) || matches!(
            self,
            Self::IoWithContext { source, .. } if is_transient_socket_kind(source.kind())
        )
    }

    pub fn diagnostic_fields(&self) -> Value {
        let mut fields = json!({
            "error": self.to_string(),
            "transient": self.is_transient_io(),
        });
        if let Some(kind) = self.io_error_kind() {
            fields["ioKind"] = json!(format!("{kind:?}"));
        }
        if let Self::IoWithContext {
            phase,
            method,
            connection_id,
            status,
            ..
        } = self
        {
            fields["phase"] = json!(phase);
            if let Some(method) = method {
                fields["method"] = json!(method);
            }
            if let Some(connection_id) = connection_id {
                fields["connectionId"] = json!(connection_id);
            }
            if let Some(status) = status {
                fields["status"] = json!(status);
            }
        }
        fields
    }

    fn io_error_kind(&self) -> Option<ErrorKind> {
        match self {
            Self::Io(error) => Some(error.kind()),
            Self::IoWithContext { source, .. } => Some(source.kind()),
            _ => None,
        }
    }
}

fn is_transient_socket_kind(kind: ErrorKind) -> bool {
    matches!(
        kind,
        ErrorKind::Interrupted
            | ErrorKind::TimedOut
            | ErrorKind::WouldBlock
            // Browsers routinely close event streams during navigation, refresh, and suspension.
            | ErrorKind::BrokenPipe
            | ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
    )
}

#[cfg(test)]
mod tests;
