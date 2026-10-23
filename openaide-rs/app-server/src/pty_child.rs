//! A process App Server runs on a pseudo-terminal, shared by Agent sign-in and Terminals.

use portable_pty::{Child, ExitStatus, PtySize};

pub(crate) fn pty_size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        cols,
        rows,
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// Owns a PTY child and its process group. Teardown is unconditional: PTY launch creates a fresh
/// session and process group, and launchers can exit while a child they started stays alive, so
/// dropping a running child kills the whole group before killing and reaping the child itself.
pub(crate) struct PtyChild {
    child: Box<dyn Child + Send + Sync>,
    running: bool,
}

impl PtyChild {
    pub(crate) fn new(child: Box<dyn Child + Send + Sync>) -> Self {
        Self {
            child,
            running: true,
        }
    }

    pub(crate) fn process_id(&self) -> Option<u32> {
        self.child.process_id()
    }

    /// Returns the exit status once the child has exited. The group is killed then, and the PID
    /// is never signalled again: it has been reaped and the OS may reuse it.
    pub(crate) fn try_exit(&mut self) -> std::io::Result<Option<ExitStatus>> {
        if !self.running {
            return Ok(None);
        }
        let Some(status) = self.child.try_wait()? else {
            return Ok(None);
        };
        self.kill_group();
        self.running = false;
        Ok(Some(status))
    }

    fn kill_group(&self) {
        #[cfg(unix)]
        if let Some(pid) = self.child.process_id() {
            let _ = nix::sys::signal::killpg(
                nix::unistd::Pid::from_raw(pid as i32),
                nix::sys::signal::Signal::SIGKILL,
            );
        }
    }
}

impl Drop for PtyChild {
    fn drop(&mut self) {
        if self.running {
            self.kill_group();
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
