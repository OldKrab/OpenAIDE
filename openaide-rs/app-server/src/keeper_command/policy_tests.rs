// timing-file: mocked — every observation passes an explicit instant; nothing waits.
use std::time::{Duration, Instant};

use super::*;

const MINUTE: Duration = Duration::from_secs(60);
const HOUR: Duration = Duration::from_secs(60 * 60);

fn idle() -> Observation {
    Observation::default()
}

fn shell_attached() -> Observation {
    Observation {
        app_shells: 1,
        ..Observation::default()
    }
}

fn waiting() -> Observation {
    Observation {
        pending_task_requests: 1,
        ..Observation::default()
    }
}

fn running() -> Observation {
    Observation {
        active_turns: 1,
        ..Observation::default()
    }
}

#[test]
fn holds_an_idle_server_for_the_grace_after_the_app_shell_left() {
    let start = Instant::now();
    let mut clock = KeeperClock::new(KeeperPolicy::default(), start);

    assert_eq!(
        clock.observe(&shell_attached(), start + MINUTE),
        Verdict::Hold(HoldReason::AppShellAttached)
    );
    assert_eq!(
        clock.observe(&idle(), start + 30 * MINUTE),
        Verdict::Hold(HoldReason::RecentAppShell)
    );
    assert_eq!(
        clock.observe(&idle(), start + 31 * MINUTE),
        Verdict::Leave(LeaveReason::Idle)
    );
}

#[test]
fn the_app_shell_that_started_the_keeper_counts_as_just_seen() {
    let start = Instant::now();
    let mut clock = KeeperClock::new(KeeperPolicy::default(), start);

    assert_eq!(
        clock.observe(&idle(), start + 29 * MINUTE),
        Verdict::Hold(HoldReason::RecentAppShell)
    );
    assert_eq!(
        clock.observe(&idle(), start + 30 * MINUTE),
        Verdict::Leave(LeaveReason::Idle)
    );
}

#[test]
fn running_work_holds_without_limit() {
    let start = Instant::now();
    let mut clock = KeeperClock::new(KeeperPolicy::default(), start);

    assert_eq!(
        clock.observe(&running(), start + 72 * HOUR),
        Verdict::Hold(HoldReason::RunningWork)
    );
}

#[test]
fn a_waiting_task_holds_for_a_day_then_the_keeper_leaves() {
    let start = Instant::now();
    let mut clock = KeeperClock::new(KeeperPolicy::default(), start);

    assert_eq!(
        clock.observe(&waiting(), start + HOUR),
        Verdict::Hold(HoldReason::WaitingForUser)
    );
    assert_eq!(
        clock.observe(&waiting(), start + 24 * HOUR + 59 * MINUTE),
        Verdict::Hold(HoldReason::WaitingForUser)
    );
    assert_eq!(
        clock.observe(&waiting(), start + 25 * HOUR),
        Verdict::Leave(LeaveReason::WaitedTooLong)
    );
}

#[test]
fn an_answered_request_restarts_the_waiting_limit() {
    let start = Instant::now();
    let mut clock = KeeperClock::new(KeeperPolicy::default(), start);

    clock.observe(&waiting(), start + HOUR);
    clock.observe(&running(), start + 20 * HOUR);

    assert_eq!(
        clock.observe(&waiting(), start + 30 * HOUR),
        Verdict::Hold(HoldReason::WaitingForUser)
    );
    assert_eq!(
        clock.observe(&waiting(), start + 54 * HOUR),
        Verdict::Leave(LeaveReason::WaitedTooLong)
    );
}

#[test]
fn an_attached_app_shell_holds_past_the_waiting_limit() {
    let start = Instant::now();
    let mut clock = KeeperClock::new(KeeperPolicy::default(), start);
    let attached_and_waiting = Observation {
        pending_task_requests: 1,
        app_shells: 1,
        ..Observation::default()
    };

    clock.observe(&attached_and_waiting, start);

    assert_eq!(
        clock.observe(&attached_and_waiting, start + 48 * HOUR),
        Verdict::Hold(HoldReason::AppShellAttached)
    );
}
