use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use serde_json::Value;

use super::{
    ClientProbeExchange, ClientProbeExchangeEndpoint, ClientProbeExchangeError,
    ClientProbeExchangeResponse,
};
use crate::storage_runtime::TransportKind;

const DEFAULT_TIMEOUT: Duration = Duration::from_millis(750);
const MAX_HEADER_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone)]
pub struct LocalHttpProbeExchange {
    timeout: Duration,
}

impl Default for LocalHttpProbeExchange {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
        }
    }
}

impl LocalHttpProbeExchange {
    pub fn with_timeout(timeout: Duration) -> Self {
        Self { timeout }
    }
}

impl ClientProbeExchange for LocalHttpProbeExchange {
    fn supports_transport(&self, transport: TransportKind) -> bool {
        transport == TransportKind::LocalHttp
    }

    fn exchange(
        &mut self,
        endpoint: ClientProbeExchangeEndpoint<'_>,
        request: Value,
    ) -> Result<ClientProbeExchangeResponse, ClientProbeExchangeError> {
        if endpoint.endpoint.endpoint.transport != TransportKind::LocalHttp {
            return Err(error("endpoint transport is not LocalHttp"));
        }

        let target = HttpTarget::parse(&endpoint.endpoint.endpoint.address)?;
        let socket_addr = target.socket_addr()?;
        if !socket_addr.ip().is_loopback() {
            return Err(error("LocalHttp endpoint must use a loopback address"));
        }

        let body = serde_json::to_string(&request)
            .map_err(|err| error(format!("client/probe request JSON failed: {err}")))?;
        let wire = http_request(&target, endpoint.endpoint.auth_token, &body);
        let mut stream = match TcpStream::connect_timeout(&socket_addr, self.timeout) {
            Ok(stream) => stream,
            Err(err) if is_unreachable_io(&err) => {
                return Ok(ClientProbeExchangeResponse::Unreachable);
            }
            Err(err) => return Err(error(format!("LocalHttp connect failed: {err}"))),
        };
        configure_timeouts(&stream, self.timeout)?;
        match write_and_read_response(&mut stream, &wire) {
            Ok(response) => Ok(response),
            Err(err) if err.is_unreachable => Ok(ClientProbeExchangeResponse::Unreachable),
            Err(err) => Err(err.error),
        }
    }
}

struct HttpTarget {
    host: String,
    port: u16,
    path: String,
}

impl HttpTarget {
    fn parse(address: &str) -> Result<Self, ClientProbeExchangeError> {
        let rest = address
            .strip_prefix("http://")
            .ok_or_else(|| error("LocalHttp endpoint must use http://"))?;
        let (authority, path) = match rest.split_once('/') {
            Some((authority, path)) => (authority, format!("/{path}")),
            None => (rest, "/".to_string()),
        };
        let (host, port) = authority
            .rsplit_once(':')
            .ok_or_else(|| error("LocalHttp endpoint must include host and port"))?;
        if host.is_empty() {
            return Err(error("LocalHttp endpoint host is empty"));
        }
        let port = port
            .parse::<u16>()
            .map_err(|err| error(format!("LocalHttp endpoint port is invalid: {err}")))?;
        Ok(Self {
            host: host.to_string(),
            port,
            path,
        })
    }

    fn socket_addr(&self) -> Result<SocketAddr, ClientProbeExchangeError> {
        format!("{}:{}", self.host, self.port)
            .parse()
            .map_err(|err| error(format!("LocalHttp socket address is invalid: {err}")))
    }
}

struct LocalHttpIoError {
    error: ClientProbeExchangeError,
    is_unreachable: bool,
}

fn configure_timeouts(
    stream: &TcpStream,
    timeout: Duration,
) -> Result<(), ClientProbeExchangeError> {
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|err| error(format!("LocalHttp read timeout setup failed: {err}")))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|err| error(format!("LocalHttp write timeout setup failed: {err}")))
}

fn write_and_read_response(
    stream: &mut TcpStream,
    wire: &str,
) -> Result<ClientProbeExchangeResponse, LocalHttpIoError> {
    stream.write_all(wire.as_bytes()).map_err(map_write_error)?;
    parse_http_response(stream)
}

fn http_request(target: &HttpTarget, auth_token: &str, body: &str) -> String {
    format!(
        "POST {} HTTP/1.1\r\nHost: {}:{}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        target.path,
        target.host,
        target.port,
        auth_token,
        body.len(),
        body
    )
}

fn parse_http_response(
    stream: &mut TcpStream,
) -> Result<ClientProbeExchangeResponse, LocalHttpIoError> {
    let mut bytes = Vec::new();
    let header_end = read_headers(stream, &mut bytes)?;
    let headers = std::str::from_utf8(&bytes[..header_end])
        .map_err(|err| hard_error(format!("LocalHttp response headers are not UTF-8: {err}")))?
        .to_string();
    let content_length = content_length(&headers)?;
    let body_start = header_end + 4;
    read_body(stream, &mut bytes, body_start, content_length)?;
    let body_end = body_start + content_length;
    let body = std::str::from_utf8(&bytes[body_start..body_end])
        .map_err(|err| hard_error(format!("LocalHttp response body is not UTF-8: {err}")))?;
    parse_http_parts(&headers, body)
}

fn read_headers(stream: &mut TcpStream, bytes: &mut Vec<u8>) -> Result<usize, LocalHttpIoError> {
    loop {
        let mut chunk = [0_u8; 512];
        let read = stream.read(&mut chunk).map_err(map_read_error)?;
        if read == 0 {
            return Err(hard_error(
                "LocalHttp response ended before headers completed",
            ));
        }
        bytes.extend_from_slice(&chunk[..read]);
        if let Some(index) = header_end_index(bytes) {
            return Ok(index);
        }
        if bytes.len() > MAX_HEADER_BYTES {
            return Err(hard_error("LocalHttp response headers are too large"));
        }
    }
}

fn read_body(
    stream: &mut TcpStream,
    bytes: &mut Vec<u8>,
    body_start: usize,
    content_length: usize,
) -> Result<(), LocalHttpIoError> {
    while bytes.len() - body_start < content_length {
        let mut chunk = [0_u8; 512];
        let read = stream.read(&mut chunk).map_err(map_read_error)?;
        if read == 0 {
            return Err(hard_error("LocalHttp response ended before body completed"));
        }
        bytes.extend_from_slice(&chunk[..read]);
    }
    Ok(())
}

fn parse_http_parts(
    headers: &str,
    body: &str,
) -> Result<ClientProbeExchangeResponse, LocalHttpIoError> {
    let status = headers
        .lines()
        .next()
        .ok_or_else(|| hard_error("LocalHttp response missing status line"))?;
    let code = status_code(status)?;
    match code {
        200 => {
            let value = serde_json::from_str::<Value>(body)
                .map_err(|err| hard_error(format!("LocalHttp response JSON failed: {err}")))?;
            Ok(ClientProbeExchangeResponse::Json(value))
        }
        401 | 403 => Ok(ClientProbeExchangeResponse::AuthFailed),
        _ => Err(hard_error(format!("LocalHttp response status {code}"))),
    }
}

fn header_end_index(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn content_length(headers: &str) -> Result<usize, LocalHttpIoError> {
    for line in headers.lines().skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("content-length") {
            return value
                .trim()
                .parse::<usize>()
                .map_err(|err| hard_error(format!("LocalHttp content length is invalid: {err}")));
        }
    }
    Err(hard_error(
        "LocalHttp response missing Content-Length header",
    ))
}

fn status_code(status: &str) -> Result<u16, LocalHttpIoError> {
    let mut parts = status.split_whitespace();
    if parts.next() != Some("HTTP/1.1") {
        return Err(hard_error("LocalHttp response must use HTTP/1.1"));
    }
    parts
        .next()
        .ok_or_else(|| hard_error("LocalHttp response missing status code"))?
        .parse::<u16>()
        .map_err(|err| hard_error(format!("LocalHttp response status is invalid: {err}")))
}

fn map_write_error(err: std::io::Error) -> LocalHttpIoError {
    if is_timeout_io(&err) {
        return unreachable_io_error("LocalHttp write timed out");
    }
    hard_error(format!("LocalHttp write failed: {err}"))
}

fn map_read_error(err: std::io::Error) -> LocalHttpIoError {
    if is_timeout_io(&err) {
        return unreachable_io_error("LocalHttp read timed out");
    }
    hard_error(format!("LocalHttp read failed: {err}"))
}

fn is_unreachable_io(err: &std::io::Error) -> bool {
    matches!(err.kind(), ErrorKind::ConnectionRefused) || is_timeout_io(err)
}

fn is_timeout_io(err: &std::io::Error) -> bool {
    matches!(err.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock)
}

fn hard_error(message: impl Into<String>) -> LocalHttpIoError {
    LocalHttpIoError {
        error: error(message),
        is_unreachable: false,
    }
}

fn unreachable_io_error(message: impl Into<String>) -> LocalHttpIoError {
    LocalHttpIoError {
        error: error(message),
        is_unreachable: true,
    }
}

fn error(message: impl Into<String>) -> ClientProbeExchangeError {
    ClientProbeExchangeError {
        message: message.into(),
    }
}

#[cfg(test)]
mod tests;
