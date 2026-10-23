//! The system `ssh` as the shared SSH launcher's transport (ADR-0069), so the
//! user's SSH configuration, jump hosts, agent, and hardware keys keep working.
//!
//! Each launch command is one `ssh` run. On macOS and Linux those runs share one
//! connection through `ControlMaster`, so a password is asked once per launch;
//! Windows' OpenSSH has no multiplexing and asks per run. The forward is its own
//! long-lived `ssh` that a supervisor restarts after a network drop, keeping the
//! same local port so the WebView's endpoint stays valid.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use openaide_remote_client::{RemoteOutput, SshTransport, TransportError};

/// Options every run shares. `ForwardX11=no` because askpass may set a
/// placeholder `DISPLAY`.
const COMMON_OPTIONS: &[&str] = &[
    "-o",
    "ConnectTimeout=20",
    "-o",
    "ServerAliveInterval=15",
    "-o",
    "ServerAliveCountMax=3",
    "-o",
    "ForwardX11=no",
];
/// Covers a person typing a password or touching a key after `ssh` starts.
const FORWARD_READY_LIMIT: Duration = Duration::from_secs(330);
const FORWARD_POLL: Duration = Duration::from_millis(100);
const RESTART_BACKOFF_LIMIT: Duration = Duration::from_secs(30);
const MAX_STDERR_BYTES: u64 = 64 * 1024;
#[cfg(target_os = "windows")]
const DETACHED_PROCESS: u32 = 0x0000_0008;

pub(crate) struct SystemSsh {
    destination: String,
    environment: Vec<(&'static str, String)>,
    control_path: Option<String>,
}

impl SystemSsh {
    /// `environment` makes `ssh` ask its questions in this Desktop.
    pub(crate) fn new(destination: &str, environment: Vec<(&'static str, String)>) -> Self {
        Self {
            destination: destination.to_string(),
            environment,
            control_path: control_path(),
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new("ssh");
        command.args(COMMON_OPTIONS);
        command.envs(self.environment.iter().map(|(name, value)| (name, value)));
        #[cfg(target_os = "windows")]
        {
            // Without a console, Windows' OpenSSH reads prompts through askpass
            // instead of a hidden console nobody can type into.
            // TODO: verify on Windows that askpass starts the helper when the
            // Desktop's install path holds spaces; only Linux has run it so far.
            use std::os::windows::process::CommandExt;
            command.creation_flags(DETACHED_PROCESS);
        }
        command
    }
}

impl SshTransport for SystemSsh {
    type Forward = SshForward;

    fn run(
        &mut self,
        remote_command: &str,
        input: Option<&mut dyn Read>,
    ) -> Result<RemoteOutput, TransportError> {
        let mut command = self.command();
        command.args(["-T", "-o", "ClearAllForwardings=yes"]);
        if let Some(path) = &self.control_path {
            command.args([
                "-o",
                "ControlMaster=auto",
                "-o",
                &format!("ControlPath={path}"),
                "-o",
                "ControlPersist=60",
            ]);
        }
        command
            .args(["--", &self.destination, remote_command])
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(spawn_failure)?;
        let stderr = collect_stderr(&mut child);
        let mut stdout = child.stdout.take().expect("piped stdout");
        let reader = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stdout.read_to_end(&mut bytes);
            bytes
        });
        if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
            // A broken pipe means `ssh` ended early; its status says why.
            let _ = std::io::copy(input, &mut stdin).and_then(|_| stdin.flush());
        }
        let status = child.wait().map_err(|_| TransportError {
            class: "ssh_failed",
        })?;
        let stdout = reader.join().unwrap_or_default();
        let stderr = stderr.join().unwrap_or_default();
        // 255 is `ssh`'s own failure; any other status is the remote command's.
        if status.code() == Some(255) {
            return Err(TransportError {
                class: classify(&stderr),
            });
        }
        Ok(RemoteOutput {
            success: status.success(),
            stdout,
        })
    }

    fn forward(&mut self, remote_port: u16) -> Result<(SshForward, SocketAddr), TransportError> {
        let local_port = free_local_port()?;
        let mut command = self.command();
        // TODO: a `LocalForward` in the user's configuration for this host
        // also binds here, and a taken port fails the forward under
        // `ExitOnForwardFailure`. `-o ClearAllForwardings=yes` would drop
        // ours too, so a fix needs `-F` with a filtered configuration.
        command.args([
            "-N",
            "-o",
            "ExitOnForwardFailure=yes",
            "-o",
            "ControlPath=none",
            "-L",
            &format!("127.0.0.1:{local_port}:127.0.0.1:{remote_port}"),
            "--",
            &self.destination,
        ]);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let local = SocketAddr::from(([127, 0, 0, 1], local_port));
        let child = start_forward(&mut command, local)?;
        Ok((SshForward::supervise(command, child, local), local))
    }
}

/// Keeps the forward up while it is held; dropping it ends the `ssh`.
pub(crate) struct SshForward {
    stopping: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
}

impl SshForward {
    fn supervise(mut command: Command, child: Child, local: SocketAddr) -> Self {
        let stopping = Arc::new(AtomicBool::new(false));
        let current = Arc::new(Mutex::new(Some(child)));
        let forward = Self {
            stopping: stopping.clone(),
            child: current.clone(),
        };
        let _ = std::thread::Builder::new()
            .name("ssh-forward".to_string())
            .spawn(move || {
                let mut attempt = 0u32;
                loop {
                    wait_for_exit(&current, &stopping);
                    if stopping.load(Ordering::Acquire) {
                        return;
                    }
                    attempt += 1;
                    let backoff = Duration::from_secs(1 << attempt.min(5)).min(RESTART_BACKOFF_LIMIT);
                    desktop_log!(
                        "desktop_ssh_forward_lost attempt_count={attempt} retry_in_ms={}",
                        backoff.as_millis()
                    );
                    if sleep_unless_stopping(backoff, &stopping) {
                        return;
                    }
                    let started = Instant::now();
                    match start_forward(&mut command, local) {
                        Ok(child) => {
                            desktop_log!(
                                "desktop_ssh_forward_restored attempt_count={attempt} duration_ms={}",
                                started.elapsed().as_millis()
                            );
                            attempt = 0;
                            if let Ok(mut slot) = current.lock() {
                                *slot = Some(child);
                            }
                        }
                        Err(error) => desktop_log!(
                            "desktop_ssh_forward_restart_failed attempt_count={attempt} error_class={} duration_ms={}",
                            error.class,
                            started.elapsed().as_millis()
                        ),
                    }
                }
            });
        forward
    }
}

impl Drop for SshForward {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        if let Ok(mut slot) = self.child.lock()
            && let Some(mut child) = slot.take()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
        desktop_log!("desktop_ssh_forward_closed");
    }
}

/// Starts the forward `ssh` and returns once the local port answers, which is
/// after any prompt and the remote's acceptance of the forward.
fn start_forward(command: &mut Command, local: SocketAddr) -> Result<Child, TransportError> {
    let started = Instant::now();
    let mut child = command.spawn().map_err(spawn_failure)?;
    let stderr = collect_stderr(&mut child);
    loop {
        if let Ok(Some(_)) = child.try_wait() {
            return Err(TransportError {
                class: match classify(&stderr.join().unwrap_or_default()) {
                    "ssh_failed" => "forward_failed",
                    class => class,
                },
            });
        }
        if TcpStream::connect_timeout(&local, FORWARD_POLL).is_ok()
            && matches!(child.try_wait(), Ok(None))
        {
            desktop_log!(
                "desktop_ssh_forward_ready wait_ms={}",
                started.elapsed().as_millis()
            );
            return Ok(child);
        }
        if started.elapsed() > FORWARD_READY_LIMIT {
            let _ = child.kill();
            let _ = child.wait();
            return Err(TransportError {
                class: "forward_timeout",
            });
        }
        std::thread::sleep(FORWARD_POLL);
    }
}

fn wait_for_exit(child: &Mutex<Option<Child>>, stopping: &AtomicBool) {
    loop {
        if stopping.load(Ordering::Acquire) {
            return;
        }
        let exited = match child.lock() {
            Ok(mut slot) => match slot.as_mut() {
                Some(child) => !matches!(child.try_wait(), Ok(None)),
                None => true,
            },
            Err(_) => true,
        };
        if exited {
            return;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// True when stopped during the wait.
fn sleep_unless_stopping(duration: Duration, stopping: &AtomicBool) -> bool {
    let until = Instant::now() + duration;
    while Instant::now() < until {
        if stopping.load(Ordering::Acquire) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    stopping.load(Ordering::Acquire)
}

/// `ssh -L` cannot choose a port and report it, so the shell picks a free one.
/// Another process taking it in between makes `ssh` fail the forward.
fn free_local_port() -> Result<u16, TransportError> {
    TcpListener::bind(("127.0.0.1", 0))
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .map_err(|_| TransportError {
            class: "local_port_unavailable",
        })
}

fn collect_stderr(child: &mut Child) -> std::thread::JoinHandle<String> {
    let stderr = child.stderr.take();
    std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(stderr) = stderr {
            let _ = stderr.take(MAX_STDERR_BYTES).read_to_string(&mut text);
        }
        text
    })
}

fn spawn_failure(error: std::io::Error) -> TransportError {
    TransportError {
        class: if error.kind() == std::io::ErrorKind::NotFound {
            "ssh_missing"
        } else {
            "ssh_failed"
        },
    }
}

/// Classifies `ssh`'s own diagnostics. The text itself names hosts and users
/// and never leaves this function.
pub(crate) fn classify(stderr: &str) -> &'static str {
    let has = |needles: &[&str]| needles.iter().any(|needle| stderr.contains(needle));
    if has(&[
        "Host key verification failed",
        "REMOTE HOST IDENTIFICATION HAS CHANGED",
    ]) {
        "host_key_rejected"
    } else if has(&["Permission denied", "Too many authentication failures"]) {
        "auth_failed"
    } else if has(&[
        "Could not resolve hostname",
        "Name or service not known",
        "nodename nor servname",
    ]) {
        "host_unknown"
    } else if has(&["Connection refused"]) {
        "connection_refused"
    } else if has(&[
        "timed out",
        "No route to host",
        "Network is unreachable",
        "Connection closed",
        "Connection reset",
    ]) {
        "unreachable"
    } else if has(&[
        "Bad configuration option",
        "Bad owner or permissions",
        "garbage at end of line",
        "Unsupported option",
    ]) {
        "ssh_config_invalid"
    } else if has(&[
        "forwarding failed",
        "cannot listen to port",
        "Address already in use",
    ]) {
        "forward_failed"
    } else {
        "ssh_failed"
    }
}

/// A place for the control socket whose directory only this user can enter,
/// short enough for a socket path, or none to run without multiplexing.
fn control_path() -> Option<String> {
    #[cfg(target_os = "macos")]
    let base = Some(std::env::temp_dir());
    #[cfg(target_os = "linux")]
    let base = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
    // TODO: multiplex on Windows too. Its OpenSSH has no ControlMaster, so a
    // password user answers once per `ssh` run; a long-lived session or an
    // embedded client, as planned for Android, would ask once per start.
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let base: Option<PathBuf> = None;
    let directory = base?.join("oa-ssh");
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        let _ = std::fs::DirBuilder::new().mode(0o700).create(&directory);
        let metadata = std::fs::metadata(&directory).ok()?;
        if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
            return None;
        }
    }
    let path = directory.join("%C").to_string_lossy().into_owned();
    // `%C` expands to 40 characters; Unix socket paths stop near 104 bytes.
    (path.len() + 40 < 100 && !path.contains(' ')).then_some(path)
}

/// A destination is passed to `ssh` as one argument after `--`; anything that
/// could read as an option or split into several words is refused.
pub(crate) fn is_destination(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 255
        && !text.starts_with('-')
        && text
            .chars()
            .all(|character| !character.is_whitespace() && !character.is_control())
}

#[cfg(test)]
#[path = "desktop_ssh_transport_tests.rs"]
mod tests;
