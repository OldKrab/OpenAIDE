//! Shows the system `ssh`'s questions in the Desktop (ADR-0069): passwords,
//! passphrases, one-time codes, and host key confirmations.
//!
//! Each `ssh` this shell starts names the Desktop's own executable as its
//! `SSH_ASKPASS` program. Started that way, the executable does not open a
//! window: it carries the one prompt over loopback to the running Desktop,
//! which asks the user and returns the answer. The listener accepts only
//! callers holding the token it handed to its `ssh` processes. Answers pass
//! through memory once and are never stored or logged.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::Emitter;

const ADDRESS_ENV: &str = "OPENAIDE_SSH_ASKPASS_ADDRESS";
const TOKEN_ENV: &str = "OPENAIDE_SSH_ASKPASS_TOKEN";
const LABEL_ENV: &str = "OPENAIDE_SSH_ASKPASS_LABEL";
pub(crate) const PROMPT_EVENT: &str = "desktop-ssh-prompt";
/// `ssh` gives up on its own connection timeout well before a person answers,
/// so this only bounds a prompt nobody is looking at.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_MESSAGE_BYTES: u64 = 16 * 1024;

/// What the helper sends: its token, `ssh`'s prompt, and `SSH_ASKPASS_PROMPT`.
#[derive(Deserialize, Serialize)]
struct Request {
    token: String,
    prompt: String,
    hint: Option<String>,
    label: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct Reply {
    answer: Option<String>,
}

/// One question as the WebView shows it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SshPrompt {
    id: u64,
    /// The SSH Computer's name in this Desktop.
    computer: String,
    /// `ssh`'s own wording, such as a password request or a host key fingerprint.
    text: String,
    kind: PromptKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PromptKind {
    /// Typed and hidden: a password, passphrase, PIN, or code.
    Secret,
    /// Answered `yes` or `no`, such as trusting a new host key.
    YesNo,
    /// Only shown, such as a request to touch a security key.
    Notice,
}

impl PromptKind {
    fn of(text: &str, hint: Option<&str>) -> Self {
        match hint {
            Some("none") => Self::Notice,
            Some("confirm") => Self::YesNo,
            _ if text.contains("(yes/no") => Self::YesNo,
            _ => Self::Secret,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Secret => "secret",
            Self::YesNo => "yes_no",
            Self::Notice => "notice",
        }
    }
}

/// A prompt shown in the App Shell and where its answer goes.
type PendingPrompt = (SshPrompt, mpsc::Sender<Option<String>>);

/// The running Desktop's half: one loopback listener for the whole process.
pub(crate) struct AskpassBroker {
    address: SocketAddr,
    token: String,
    next_id: AtomicU64,
    pending: Mutex<HashMap<u64, PendingPrompt>>,
}

impl AskpassBroker {
    pub(crate) fn start(app: tauri::AppHandle) -> std::io::Result<Arc<Self>> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let broker = Arc::new(Self {
            address: listener.local_addr()?,
            token: format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            ),
            next_id: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
        });
        let serving = broker.clone();
        std::thread::Builder::new()
            .name("ssh-askpass".to_string())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    let broker = serving.clone();
                    let app = app.clone();
                    let _ = std::thread::Builder::new()
                        .name("ssh-askpass-prompt".to_string())
                        .spawn(move || broker.serve(&app, stream));
                }
            })?;
        desktop_log!("desktop_ssh_askpass_started");
        Ok(broker)
    }

    /// The environment that makes an `ssh` ask this Desktop.
    pub(crate) fn environment(&self, computer: &str) -> Vec<(&'static str, String)> {
        let program = std::env::current_exe()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut environment = vec![
            ("SSH_ASKPASS", program),
            // OpenSSH 8.4 and later use askpass even with a terminal attached.
            ("SSH_ASKPASS_REQUIRE", "force".to_string()),
            (ADDRESS_ENV, self.address.to_string()),
            (TOKEN_ENV, self.token.clone()),
            (LABEL_ENV, computer.to_string()),
        ];
        // Older OpenSSH, including Windows 10's, uses askpass only with a display.
        if std::env::var_os("DISPLAY").is_none() {
            environment.push(("DISPLAY", "openaide:0".to_string()));
        }
        environment
    }

    pub(crate) fn pending(&self) -> Vec<SshPrompt> {
        let mut prompts: Vec<SshPrompt> = self
            .pending
            .lock()
            .map(|pending| pending.values().map(|(prompt, _)| prompt.clone()).collect())
            .unwrap_or_default();
        prompts.sort_by_key(|prompt| prompt.id);
        prompts
    }

    /// Delivers the user's answer; `None` declines.
    pub(crate) fn answer(&self, id: u64, answer: Option<String>) -> bool {
        let entry = self
            .pending
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(&id));
        match entry {
            Some((_, reply)) => reply.send(answer).is_ok(),
            None => false,
        }
    }

    fn serve(&self, app: &tauri::AppHandle, stream: TcpStream) {
        let started = Instant::now();
        let Some(request) = read_message::<Request>(&stream) else {
            desktop_log!("desktop_ssh_prompt_refused reason=unreadable");
            return;
        };
        if !same_token(&request.token, &self.token) {
            desktop_log!("desktop_ssh_prompt_refused reason=token");
            return;
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let kind = PromptKind::of(&request.prompt, request.hint.as_deref());
        let prompt = SshPrompt {
            id,
            computer: request.label.unwrap_or_default(),
            text: request.prompt,
            kind,
        };
        desktop_log!(
            "desktop_ssh_prompt_started prompt_id={id} kind={}",
            kind.as_str()
        );
        let (sender, receiver) = mpsc::channel();
        if kind != PromptKind::Notice
            && let Ok(mut pending) = self.pending.lock()
        {
            pending.insert(id, (prompt.clone(), sender));
        }
        let _ = app.emit(PROMPT_EVENT, &prompt);
        // A notice needs no answer: `ssh` ends the helper when it is done.
        let (answer, outcome) = if kind == PromptKind::Notice {
            (None, "shown")
        } else {
            match receiver.recv_timeout(ANSWER_TIMEOUT) {
                Ok(Some(answer)) => (Some(answer), "answered"),
                Ok(None) => (None, "declined"),
                Err(_) => {
                    if let Ok(mut pending) = self.pending.lock() {
                        pending.remove(&id);
                    }
                    (None, "timed_out")
                }
            }
        };
        let _ = write_message(&stream, &Reply { answer });
        desktop_log!(
            "desktop_ssh_prompt_completed prompt_id={id} outcome={outcome} duration_ms={}",
            started.elapsed().as_millis()
        );
    }
}

/// Runs the helper when `ssh` started this executable as its askpass program.
/// Returns the exit code, or `None` for a normal Desktop start.
pub(crate) fn run_helper_if_requested() -> Option<i32> {
    let address = std::env::var(ADDRESS_ENV).ok()?;
    let token = std::env::var(TOKEN_ENV).unwrap_or_default();
    let prompt = std::env::args().nth(1).unwrap_or_default();
    let request = Request {
        token,
        prompt,
        hint: std::env::var("SSH_ASKPASS_PROMPT").ok(),
        label: std::env::var(LABEL_ENV).ok(),
    };
    let answer = address
        .parse::<SocketAddr>()
        .ok()
        .and_then(|address| TcpStream::connect_timeout(&address, Duration::from_secs(5)).ok())
        .and_then(|stream| {
            write_message(&stream, &request).ok()?;
            read_message::<Reply>(&stream)
        })
        .and_then(|reply| reply.answer);
    match answer {
        Some(answer) => {
            let mut stdout = std::io::stdout();
            let _ = writeln!(stdout, "{answer}");
            let _ = stdout.flush();
            Some(0)
        }
        None => Some(1),
    }
}

fn read_message<T: for<'de> Deserialize<'de>>(stream: &TcpStream) -> Option<T> {
    let mut line = String::new();
    BufReader::new(stream.take(MAX_MESSAGE_BYTES))
        .read_line(&mut line)
        .ok()?;
    serde_json::from_str(&line).ok()
}

fn write_message<T: Serialize>(mut stream: &TcpStream, message: &T) -> std::io::Result<()> {
    let mut bytes = serde_json::to_vec(message).map_err(std::io::Error::other)?;
    bytes.push(b'\n');
    stream.write_all(&bytes)
}

fn same_token(given: &str, expected: &str) -> bool {
    given.len() == expected.len()
        && given
            .bytes()
            .zip(expected.bytes())
            .fold(0u8, |difference, (left, right)| difference | (left ^ right))
            == 0
}

#[cfg(test)]
#[path = "desktop_ssh_askpass_tests.rs"]
mod tests;
