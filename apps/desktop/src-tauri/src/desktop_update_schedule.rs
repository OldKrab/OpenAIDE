use std::path::Path;

use serde::{Deserialize, Serialize};

const CHECK_INTERVAL_MS: u64 = 24 * 60 * 60 * 1_000;
const MAX_JITTER_MS: u64 = 15 * 60 * 1_000;
const INITIAL_BACKOFF_MS: u64 = 15 * 60 * 1_000;
const MAX_BACKOFF_MS: u64 = 6 * 60 * 60 * 1_000;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DesktopUpdateSchedule {
    pub(crate) last_attempt_at_ms: Option<u64>,
    pub(crate) last_success_at_ms: Option<u64>,
    pub(crate) next_auto_check_at_ms: u64,
    pub(crate) failure_count: u8,
}

pub(crate) fn auto_check_due(path: &Path, now: u64) -> bool {
    now >= read_schedule(path).next_auto_check_at_ms
}

/// Persists a short crash backoff before Desktop crosses the network boundary.
pub(crate) fn record_check_started(path: &Path, now: u64) {
    let mut schedule = read_schedule(path);
    schedule.last_attempt_at_ms = Some(now);
    schedule.next_auto_check_at_ms = now + INITIAL_BACKOFF_MS + check_jitter(now);
    let _ = write_schedule(path, &schedule);
}

pub(crate) fn record_check_terminal(path: &Path, now: u64, success: bool) {
    let mut schedule = read_schedule(path);
    schedule.last_attempt_at_ms = Some(now);
    if success {
        schedule.last_success_at_ms = Some(now);
        schedule.failure_count = 0;
        schedule.next_auto_check_at_ms = now + CHECK_INTERVAL_MS + check_jitter(now);
    } else {
        schedule.failure_count = schedule.failure_count.saturating_add(1).min(6);
        let multiplier = 1_u64 << schedule.failure_count.saturating_sub(1);
        let backoff = (INITIAL_BACKOFF_MS * multiplier).min(MAX_BACKOFF_MS);
        schedule.next_auto_check_at_ms = now + backoff + check_jitter(now);
    }
    let _ = write_schedule(path, &schedule);
}

pub(crate) fn read_schedule(path: &Path) -> DesktopUpdateSchedule {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn write_schedule(path: &Path, schedule: &DesktopUpdateSchedule) -> Result<(), ()> {
    let parent = path.parent().ok_or(())?;
    std::fs::create_dir_all(parent).map_err(|_| ())?;
    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec(schedule).map_err(|_| ())?;
    std::fs::write(&temporary, bytes).map_err(|_| ())?;
    std::fs::rename(temporary, path).map_err(|_| ())
}

fn check_jitter(now: u64) -> u64 {
    now.rotate_left(17) % MAX_JITTER_MS
}
