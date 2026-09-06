use std::process::{Child, ChildStdout};

/// Owns a newly launched process until its startup handoff has been accepted.
/// Success transfers supervision back to the caller; every earlier return must
/// stop and reap the child rather than abandoning it after malformed startup data.
pub(crate) struct StartupChild<'a> {
    child: &'a mut Child,
    accepted: bool,
}

impl<'a> StartupChild<'a> {
    pub(crate) fn new(child: &'a mut Child) -> Self {
        Self {
            child,
            accepted: false,
        }
    }

    pub(crate) fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.stdout.take()
    }

    pub(crate) fn accept(mut self) {
        self.accepted = true;
    }
}

impl Drop for StartupChild<'_> {
    fn drop(&mut self) {
        if !self.accepted {
            let _ = self.child.kill();
            // kill alone leaves an exited child unreaped on Unix. Keep failure
            // cleanup synchronous so the next bootstrap starts with no old owner.
            let _ = self.child.wait();
        }
    }
}

#[cfg(test)]
#[path = "startup_process_tests.rs"]
mod tests;
