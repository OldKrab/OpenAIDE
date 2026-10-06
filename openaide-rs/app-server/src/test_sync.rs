//! Shared synchronization budget for this crate's tests.

use std::time::Duration;

/// Deadlock watchdog for a wait on an outcome the test has already made
/// inevitable. It only bounds a hung test and never establishes ordering, so
/// it is far longer than a loaded runner needs. Windows that assert something
/// does *not* happen, and elapsed-time contracts, keep their own durations.
pub(crate) const WATCHDOG: Duration = Duration::from_secs(30);
