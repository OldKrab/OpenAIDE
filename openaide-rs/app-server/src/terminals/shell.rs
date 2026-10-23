//! One Terminal's shell process, its retained output, and the clients viewing it (ADR-0071).
//!
//! Three threads serve each shell: a reader that copies PTY output into the replay and to every
//! viewer, a writer that feeds input so a shell that stops reading never blocks a socket, and a
//! monitor that reports status changes. None of them holds the protocol lock.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use openaide_app_server_protocol::ids::ProjectId;
use openaide_app_server_protocol::terminals::{TerminalStatus, TerminalSummary};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty};

use super::replay::Replay;
use crate::pty_child::{pty_size, PtyChild};

/// Output chunks a viewer may fall behind by before it is dropped and must reattach. Dropping
/// keeps one slow connection from stalling the shell or the other viewers.
const VIEWER_BACKLOG_CHUNKS: usize = 256;
const READ_CHUNK_BYTES: usize = 16 * 1024;
/// Status is polled because no portable notification exists for a foreground process change.
const MONITOR_INTERVAL: Duration = Duration::from_millis(500);
pub(crate) const MAX_COLS: u16 = 500;
pub(crate) const MAX_ROWS: u16 = 200;

pub(crate) fn valid_size(cols: u16, rows: u16) -> bool {
    (2..=MAX_COLS).contains(&cols) && (2..=MAX_ROWS).contains(&rows)
}

/// What a newly attached viewer receives: the replay, then live output until the channel closes.
pub(crate) struct ViewerAttach {
    pub(crate) replay: Vec<u8>,
    pub(crate) output: Receiver<Vec<u8>>,
}

struct Viewer {
    id: u64,
    output: SyncSender<Vec<u8>>,
}

#[derive(Clone, PartialEq, Eq)]
struct Status {
    status: TerminalStatus,
    command: Option<String>,
    folder: String,
    exit_code: Option<i32>,
}

struct Output {
    replay: Replay,
    viewers: Vec<Viewer>,
    next_viewer_id: u64,
    /// No further output will arrive; attaches receive the replay and end.
    ended: bool,
}

pub(crate) struct Shell {
    pub(crate) terminal_id: String,
    pub(crate) project_id: ProjectId,
    opened_at: u64,
    status: Mutex<Status>,
    output: Mutex<Output>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    /// Taken when the Terminal closes, which kills the process group.
    child: Mutex<Option<PtyChild>>,
    input: SyncSender<Vec<u8>>,
    closed: AtomicBool,
}

impl Shell {
    /// Starts the user's login shell in `folder`. `changed` runs on the monitor thread after the
    /// published status changes.
    pub(crate) fn spawn(
        terminal_id: String,
        project_id: ProjectId,
        folder: &Path,
        cols: u16,
        rows: u16,
        opened_at: u64,
        changed: impl Fn(&Shell) + Send + 'static,
    ) -> std::io::Result<Arc<Self>> {
        let pair = native_pty_system()
            .openpty(pty_size(cols, rows))
            .map_err(std::io::Error::other)?;
        let child = PtyChild::new(
            pair.slave
                .spawn_command(login_shell(folder))
                .map_err(std::io::Error::other)?,
        );
        // The parent's copy of the slave would keep the PTY open after the shell exits.
        drop(pair.slave);
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(std::io::Error::other)?;
        let mut writer = pair.master.take_writer().map_err(std::io::Error::other)?;
        let (input, input_receive) = mpsc::sync_channel::<Vec<u8>>(64);
        std::thread::spawn(move || {
            while let Ok(bytes) = input_receive.recv() {
                if writer
                    .write_all(&bytes)
                    .and_then(|_| writer.flush())
                    .is_err()
                {
                    break;
                }
            }
        });
        let shell = Arc::new(Self {
            terminal_id,
            project_id,
            opened_at,
            status: Mutex::new(Status {
                status: TerminalStatus::Idle,
                command: None,
                folder: folder.display().to_string(),
                exit_code: None,
            }),
            output: Mutex::new(Output {
                replay: Replay::default(),
                viewers: Vec::new(),
                next_viewer_id: 0,
                ended: false,
            }),
            master: Mutex::new(pair.master),
            child: Mutex::new(Some(child)),
            input,
            closed: AtomicBool::new(false),
        });
        let reading = shell.clone();
        std::thread::spawn(move || reading.read_output(reader));
        let monitored = shell.clone();
        std::thread::spawn(move || monitored.monitor(changed));
        Ok(shell)
    }

    pub(crate) fn summary(&self) -> TerminalSummary {
        let status = self
            .status
            .lock()
            .expect("terminal status poisoned")
            .clone();
        TerminalSummary {
            terminal_id: self.terminal_id.clone(),
            project_id: self.project_id.clone(),
            folder: status.folder,
            command: status.command,
            status: status.status,
            exit_code: status.exit_code,
            opened_at: self.opened_at,
        }
    }

    pub(crate) fn is_busy(&self) -> bool {
        self.status.lock().expect("terminal status poisoned").status == TerminalStatus::Busy
    }

    pub(crate) fn viewer_count(&self) -> usize {
        self.output
            .lock()
            .expect("terminal output poisoned")
            .viewers
            .len()
    }

    /// Registers a viewer under the output lock, so the replay and live output neither overlap
    /// nor leave a gap.
    pub(crate) fn attach(&self) -> (u64, ViewerAttach) {
        let mut output = self.output.lock().expect("terminal output poisoned");
        let (sender, receiver) = mpsc::sync_channel(VIEWER_BACKLOG_CHUNKS);
        let id = output.next_viewer_id;
        output.next_viewer_id += 1;
        if !output.ended {
            output.viewers.push(Viewer { id, output: sender });
        }
        (
            id,
            ViewerAttach {
                replay: output.replay.contents(),
                output: receiver,
            },
        )
    }

    /// Tells a viewer whose channel closed whether the shell ended or the viewer fell behind.
    pub(crate) fn output_ended(&self) -> bool {
        self.output.lock().expect("terminal output poisoned").ended
    }

    pub(crate) fn detach(&self, viewer_id: u64) {
        self.output
            .lock()
            .expect("terminal output poisoned")
            .viewers
            .retain(|viewer| viewer.id != viewer_id);
    }

    /// Blocks only the calling socket when the shell stops reading input.
    pub(crate) fn write_input(&self, bytes: Vec<u8>) -> bool {
        !self.closed.load(Ordering::Acquire) && self.input.send(bytes).is_ok()
    }

    pub(crate) fn resize(&self, cols: u16, rows: u16) -> bool {
        self.master
            .lock()
            .expect("terminal master poisoned")
            .resize(pty_size(cols, rows))
            .is_ok()
    }

    /// Ends the shell and its process group. Viewer channels close, which ends their sockets.
    pub(crate) fn terminate(&self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        // Dropping a running child kills its group and reaps it.
        drop(self.child.lock().expect("terminal child poisoned").take());
        let mut output = self.output.lock().expect("terminal output poisoned");
        output.ended = true;
        output.viewers.clear();
    }

    fn read_output(&self, mut reader: Box<dyn Read + Send>) {
        let mut bytes = vec![0_u8; READ_CHUNK_BYTES];
        loop {
            // EOF or EIO once the shell and everything holding the PTY have exited.
            let count = match reader.read(&mut bytes) {
                Ok(0) | Err(_) => break,
                Ok(count) => count,
            };
            let chunk = &bytes[..count];
            let mut output = self.output.lock().expect("terminal output poisoned");
            output.replay.push(chunk);
            let before = output.viewers.len();
            output
                .viewers
                .retain(|viewer| match viewer.output.try_send(chunk.to_vec()) {
                    Ok(()) => true,
                    Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => false,
                });
            let dropped = before - output.viewers.len();
            drop(output);
            if dropped > 0 {
                crate::logging::info(
                    "terminal_viewer_dropped",
                    serde_json::json!({
                        "terminal_id": self.terminal_id,
                        "viewers_dropped": dropped,
                        "reason_code": "viewer_behind",
                    }),
                );
            }
        }
        let mut output = self.output.lock().expect("terminal output poisoned");
        output.ended = true;
        output.viewers.clear();
    }

    fn monitor(&self, changed: impl Fn(&Shell)) {
        while !self.closed.load(Ordering::Acquire) {
            std::thread::sleep(MONITOR_INTERVAL);
            if self.closed.load(Ordering::Acquire) {
                return;
            }
            let exit = self
                .child
                .lock()
                .expect("terminal child poisoned")
                .as_mut()
                .and_then(|child| child.try_exit().ok().flatten());
            let next = match exit.as_ref() {
                Some(exit) => Status {
                    status: TerminalStatus::Exited,
                    command: None,
                    exit_code: Some(i32::try_from(exit.exit_code()).unwrap_or(i32::MAX)),
                    folder: self
                        .status
                        .lock()
                        .expect("terminal status poisoned")
                        .folder
                        .clone(),
                },
                None => self.observe(),
            };
            let mut status = self.status.lock().expect("terminal status poisoned");
            if *status == next {
                continue;
            }
            *status = next;
            drop(status);
            changed(self);
            if exit.is_some() {
                return;
            }
        }
    }

    /// Reads the foreground command and the shell's folder where the platform exposes them.
    fn observe(&self) -> Status {
        let shell_pid = self
            .child
            .lock()
            .expect("terminal child poisoned")
            .as_ref()
            .and_then(PtyChild::process_id);
        let previous = self
            .status
            .lock()
            .expect("terminal status poisoned")
            .clone();
        #[cfg(unix)]
        {
            let foreground = self
                .master
                .lock()
                .expect("terminal master poisoned")
                .process_group_leader()
                .and_then(|pid| u32::try_from(pid).ok());
            let busy = matches!((foreground, shell_pid), (Some(fg), Some(shell)) if fg != shell);
            Status {
                status: if busy {
                    TerminalStatus::Busy
                } else {
                    TerminalStatus::Idle
                },
                command: foreground.filter(|_| busy).and_then(platform::command_name),
                folder: shell_pid
                    .and_then(platform::current_folder)
                    .map(|folder| folder.display().to_string())
                    .unwrap_or(previous.folder),
                exit_code: None,
            }
        }
        // A ConPTY has no foreground process group, and App Server is not attached to the
        // pseudo console, so a program the shell started is the sign of a running command.
        // PowerShell runs its own cmdlets in-process, so those count as idle.
        #[cfg(windows)]
        {
            let command = shell_pid.and_then(platform::running_child);
            Status {
                status: if command.is_some() {
                    TerminalStatus::Busy
                } else {
                    TerminalStatus::Idle
                },
                command,
                // TODO(terminals): follow the shell's folder on Windows; PowerShell's location
                // is not its process's current directory, so it needs the shell's cooperation.
                folder: previous.folder,
                exit_code: None,
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = shell_pid;
            previous
        }
    }
}

/// The user's login shell, run in the App Server's environment (ADR-0071).
fn login_shell(folder: &Path) -> CommandBuilder {
    #[cfg(unix)]
    let mut command = {
        let shell = std::env::var_os("SHELL")
            .filter(|shell| !shell.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/bin/sh"));
        let mut command = CommandBuilder::new(shell);
        command.arg("-l");
        command
    };
    #[cfg(not(unix))]
    let mut command = {
        let mut command = CommandBuilder::new("powershell.exe");
        command.arg("-NoLogo");
        command
    };
    command.cwd(folder);
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command
}

#[cfg(unix)]
mod platform {
    use std::path::PathBuf;

    #[cfg(target_os = "linux")]
    pub(super) fn command_name(pid: u32) -> Option<String> {
        let name = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
        Some(name.trim_end().to_string()).filter(|name| !name.is_empty())
    }

    #[cfg(target_os = "linux")]
    pub(super) fn current_folder(pid: u32) -> Option<PathBuf> {
        std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
    }

    // TODO(terminals): read the foreground command and folder on macOS through libproc; until
    // then a busy Terminal shows its folder as the title and the start folder stays displayed.
    #[cfg(not(target_os = "linux"))]
    pub(super) fn command_name(_pid: u32) -> Option<String> {
        None
    }

    #[cfg(not(target_os = "linux"))]
    pub(super) fn current_folder(_pid: u32) -> Option<PathBuf> {
        None
    }
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    /// The program name of the shell's most recently listed child process, if it has one.
    pub(super) fn running_child(shell_pid: u32) -> Option<String> {
        // SAFETY: the snapshot handle is checked and closed here, and each entry is a
        // correctly sized PROCESSENTRY32W that the API fills in.
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return None;
            }
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let mut child = None;
            let mut more = Process32FirstW(snapshot, &mut entry) != 0;
            while more {
                if entry.th32ParentProcessID == shell_pid && entry.th32ProcessID != shell_pid {
                    let name = &entry.szExeFile;
                    let length = name
                        .iter()
                        .position(|unit| *unit == 0)
                        .unwrap_or(name.len());
                    child = Some(String::from_utf16_lossy(&name[..length]));
                }
                more = Process32NextW(snapshot, &mut entry) != 0;
            }
            CloseHandle(snapshot);
            child.map(|name| program_name(&name))
        }
    }

    fn program_name(executable: &str) -> String {
        let length = executable.len();
        if length > 4 && executable[length - 4..].eq_ignore_ascii_case(".exe") {
            executable[..length - 4].to_string()
        } else {
            executable.to_string()
        }
    }
}
