use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::json;

use super::*;

#[test]
fn only_read_only_requests_that_already_run_off_the_protocol_lock_are_concurrent() {
    for method in [
        FILE_VIEWER_LIST_DIRECTORY,
        FILE_VIEWER_SEARCH,
        FILE_VIEWER_EXISTING_FILES,
        FILE_VIEWER_CHANGES,
        FILE_VIEWER_DIFF,
        TASK_IMAGE,
        TASK_NAVIGATION_SEARCH,
    ] {
        assert!(is_concurrent_read(method), "{method}");
    }
    // Stateful or ordering-sensitive requests stay on the ordered path.
    for method in [
        "state/subscribe",
        "task/open",
        "task/send",
        "fileViewer/open",
        "fileViewer/release",
        "client/initialize",
    ] {
        assert!(!is_concurrent_read(method), "{method}");
    }
}

#[test]
fn only_a_request_with_an_id_names_a_concurrent_method() {
    let request = json!({"jsonrpc": "2.0", "id": 1, "method": FILE_VIEWER_CHANGES});
    assert_eq!(
        request_method(&request).as_deref(),
        Some(FILE_VIEWER_CHANGES)
    );
    let notification = json!({"jsonrpc": "2.0", "method": FILE_VIEWER_CHANGES});
    assert_eq!(request_method(&notification), None);
    let wrong_version = json!({"jsonrpc": "1.0", "id": 1, "method": FILE_VIEWER_CHANGES});
    assert_eq!(request_method(&wrong_version), None);
}

#[test]
fn a_full_limiter_holds_the_next_read_until_a_slot_frees() {
    let reads = ConcurrentReads::with_limit(2);
    let first = reads.acquire(TASK_IMAGE);
    let _second = reads.acquire(TASK_IMAGE);
    assert_eq!(reads.in_flight(), 2);

    let acquired = Arc::new(AtomicBool::new(false));
    let waiter = {
        let (reads, acquired) = (reads.clone(), acquired.clone());
        std::thread::spawn(move || {
            let _slot = reads.acquire(TASK_IMAGE);
            acquired.store(true, Ordering::SeqCst);
        })
    };
    // The limiter state itself excludes the third read; the window only observes it.
    crate::test_sync::observe_absence();
    assert!(!acquired.load(Ordering::SeqCst));

    drop(first);
    crate::test_sync::wait_until("the waiting read gets the freed slot", || {
        acquired.load(Ordering::SeqCst)
    });
    waiter.join().unwrap();
    crate::test_sync::wait_until("every slot is released", || reads.in_flight() == 1);
}
