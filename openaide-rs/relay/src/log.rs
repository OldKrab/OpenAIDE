//! Structured diagnostics: one JSON object per line on standard error, which journald
//! collects for the service.
//!
//! Events carry metadata only. They never hold a token, a key line, a person's name, or
//! free-form error text: a key appears as [`key_id`], and an error as a stable class.
//! Command output meant for people goes to standard output instead, in plain words.

use std::io::{self, Write};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use tracing::Subscriber;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

/// Writes one event. `fields` must be a JSON object; its entries follow `ts` and `event`.
pub fn event(name: &str, fields: Value) {
    let mut line = Map::new();
    line.insert("ts".into(), json!(unix_millis()));
    line.insert("event".into(), json!(name));
    if let Value::Object(fields) = fields {
        line.extend(fields);
    }
    let mut text = Value::Object(line).to_string();
    text.push('\n');
    if cfg!(test) {
        // The test harness captures `eprint!` and shows it only for a failing test.
        eprint!("{text}");
    } else {
        // A closed standard error must not take the relay down with it.
        let _ = io::stderr().lock().write_all(text.as_bytes());
    }
}

/// Milliseconds since `started`, for an event's `durationMs`.
pub fn millis_since(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// A short, stable identifier for a relay key that does not reveal it: the first 8 hex
/// characters of the token's SHA-256.
pub fn key_id(token: &str) -> String {
    short_hex(&Sha256::digest(token.as_bytes()))
}

pub(crate) fn short_hex(digest: &[u8]) -> String {
    digest
        .iter()
        .take(4)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A stable class for an I/O failure, safe to log in place of its message.
pub fn io_class(error: &io::Error) -> &'static str {
    match error.kind() {
        io::ErrorKind::NotFound => "not_found",
        io::ErrorKind::PermissionDenied => "permission_denied",
        io::ErrorKind::AlreadyExists => "already_exists",
        io::ErrorKind::AddrInUse => "address_in_use",
        io::ErrorKind::AddrNotAvailable => "address_not_available",
        io::ErrorKind::InvalidData => "invalid_data",
        io::ErrorKind::TimedOut => "timed_out",
        io::ErrorKind::StorageFull => "storage_full",
        io::ErrorKind::ReadOnlyFilesystem => "read_only_filesystem",
        _ => "io",
    }
}

/// Forwards iroh's own warnings and errors as metadata-only events: the level, the module,
/// and the source line that raised it. Their messages may hold addresses or error text, so
/// they are dropped; the source line is enough to find the cause in iroh's code.
struct IrohDiagnostics;

impl<S: Subscriber> Layer<S> for IrohDiagnostics {
    fn on_event(&self, event: &tracing::Event<'_>, _context: Context<'_, S>) {
        let metadata = event.metadata();
        self::event(
            "iroh.diagnostic",
            json!({
                "level": metadata.level().as_str(),
                "target": metadata.target(),
                "site": format!("{}:{}", metadata.file().unwrap_or("?"), metadata.line().unwrap_or(0)),
            }),
        );
    }
}

/// Installs [`IrohDiagnostics`] for warnings and errors. Only `serve` calls this.
pub fn install_iroh_diagnostics() {
    let subscriber =
        tracing_subscriber::registry().with(IrohDiagnostics.with_filter(LevelFilter::WARN));
    // A second install (as in tests) keeps the first subscriber.
    let _ = tracing::subscriber::set_global_default(subscriber);
}
