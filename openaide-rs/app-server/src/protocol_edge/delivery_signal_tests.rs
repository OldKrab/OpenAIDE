use std::sync::mpsc;

use super::*;

#[test]
fn a_notification_wakes_a_waiter_before_its_timeout() {
    let signal = DeliverySignal::default();
    let seen = signal.generation();
    let (waiting, started) = mpsc::channel();
    let waiter = {
        let signal = signal.clone();
        std::thread::spawn(move || {
            waiting.send(()).expect("test receiver is alive");
            signal.wait_changed(seen, crate::test_sync::WATCHDOG)
        })
    };
    started.recv().expect("waiter started");

    signal.notify();

    assert_ne!(waiter.join().expect("waiter completes"), seen);
}

#[test]
fn a_notification_before_the_wait_is_not_lost() {
    let signal = DeliverySignal::default();
    let seen = signal.generation();

    signal.notify();

    assert_ne!(signal.wait_changed(seen, crate::test_sync::WATCHDOG), seen);
}

#[test]
fn a_quiet_wait_returns_the_unchanged_generation_at_its_timeout() {
    let signal = DeliverySignal::default();
    let seen = signal.generation();

    assert_eq!(signal.wait_changed(seen, crate::test_sync::EXPIRES), seen);
}
