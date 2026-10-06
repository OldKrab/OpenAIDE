use std::time::Duration;

/// When an inactive attachment closes its Native Session, and how long that
/// autonomous close may stay unanswered before the attachment ends without it.
/// It is a live value so a change applies to attachments that are already open.
#[derive(Clone, Copy)]
pub(in crate::agent) struct SessionIdleTimeouts {
    pub(in crate::agent) idle: Duration,
    pub(in crate::agent) close: Duration,
}

impl Default for SessionIdleTimeouts {
    fn default() -> Self {
        Self {
            idle: Duration::from_secs(30 * 60),
            close: Duration::from_secs(2),
        }
    }
}
