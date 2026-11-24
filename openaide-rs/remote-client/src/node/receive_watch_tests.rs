// timing-file: data — the watch takes explicit instants; nothing waits.

use super::*;

const STALL: Duration = Duration::from_secs(12);

fn at(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

#[test]
fn a_connection_that_keeps_receiving_is_never_reported() {
    let mut watch = ReceiveWatch::new(STALL);
    for second in 0..=60 {
        assert_eq!(watch.sample(at(second * 1_000), second, true), None);
    }
}

#[test]
fn nothing_received_while_on_screen_is_reported_once_then_its_end() {
    let mut watch = ReceiveWatch::new(STALL);
    assert_eq!(watch.sample(at(0), 10, true), None);
    assert_eq!(watch.sample(STALL - at(1), 10, true), None);
    assert_eq!(watch.sample(STALL, 10, true), Some(Change::Stalled(STALL)));
    // Still stalled: already reported.
    assert_eq!(watch.sample(STALL * 3, 10, true), None);
    assert_eq!(
        watch.sample(STALL * 4, 11, true),
        Some(Change::Resumed(STALL * 4))
    );
    assert_eq!(watch.sample(STALL * 5, 12, true), None);
}

#[test]
fn silence_while_hidden_is_not_a_stall() {
    // A phone stops a hidden app's network; the return replaces the endpoint anyway.
    let mut watch = ReceiveWatch::new(STALL);
    assert_eq!(watch.sample(at(0), 10, true), None);
    assert_eq!(watch.sample(STALL * 2, 10, false), None);
    // Back on screen: the wait starts over from the return.
    assert_eq!(watch.sample(STALL * 2 + at(1), 10, true), None);
    assert_eq!(watch.sample(STALL * 3, 10, true), None);
    assert_eq!(
        watch.sample(STALL * 3 + at(1), 10, true),
        Some(Change::Stalled(STALL))
    );
}

#[test]
fn hiding_during_a_stall_ends_its_report() {
    let mut watch = ReceiveWatch::new(STALL);
    assert_eq!(watch.sample(at(0), 10, true), None);
    assert_eq!(watch.sample(STALL, 10, true), Some(Change::Stalled(STALL)));
    assert_eq!(
        watch.sample(STALL + at(500), 10, false),
        Some(Change::Hidden(STALL + at(500)))
    );
}

#[test]
fn a_connection_that_ends_during_a_stall_reports_how_long_it_lasted() {
    let mut watch = ReceiveWatch::new(STALL);
    assert_eq!(watch.sample(at(0), 10, true), None);
    assert_eq!(watch.sample(STALL, 10, true), Some(Change::Stalled(STALL)));
    assert_eq!(watch.end(STALL * 2), Some(Change::Ended(STALL * 2)));
    assert_eq!(ReceiveWatch::new(STALL).end(at(1_000)), None);
}
