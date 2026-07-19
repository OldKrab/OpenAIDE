use serde_json::json;
use std::time::Duration;

use crate::client_lifecycle::ConnectionId;

use super::*;

#[test]
fn explicit_close_removes_only_the_matching_connection_session() {
    let sessions = ReliableSessionRegistry::new("server-1");
    let opened = sessions.open(ConnectionId::new("generation-1"));

    assert!(!sessions.close(&opened.session_id, &ConnectionId::new("generation-other")));
    assert!(sessions.close(&opened.session_id, &ConnectionId::new("generation-1")));
    assert_eq!(
        sessions.poll(&opened.session_id, 0),
        Err(PollError::UnknownSession)
    );
}

#[test]
fn opening_after_idle_timeout_removes_lost_close_sessions_and_replay() {
    let sessions =
        ReliableSessionRegistry::with_idle_timeout("server-1", Duration::from_millis(10));
    let abandoned = sessions.open(ConnectionId::new("generation-1"));
    sessions.enqueue_server_message(&abandoned.session_id, json!({ "large": "replay" }));
    std::thread::sleep(Duration::from_millis(15));

    let current = sessions.open(ConnectionId::new("generation-2"));

    assert_eq!(
        sessions.poll(&abandoned.session_id, 0),
        Err(PollError::UnknownSession)
    );
    assert_eq!(
        sessions.connection_id(&current.session_id),
        Some(ConnectionId::new("generation-2"))
    );
}

#[test]
fn a_later_poll_lookup_expires_a_lost_close_without_another_open() {
    let sessions =
        ReliableSessionRegistry::with_idle_timeout("server-1", Duration::from_millis(10));
    let abandoned = sessions.open(ConnectionId::new("generation-1"));
    sessions.enqueue_server_message(&abandoned.session_id, json!({ "large": "replay" }));
    std::thread::sleep(Duration::from_millis(15));

    assert_eq!(sessions.connection_id(&abandoned.session_id), None);
    assert_eq!(sessions.session_count(), 0);
}

#[test]
fn idle_expiry_runs_without_another_open_poll_or_lookup() {
    let sessions =
        ReliableSessionRegistry::with_idle_timeout("server-1", Duration::from_millis(10));
    let abandoned = sessions.open(ConnectionId::new("generation-1"));
    sessions.enqueue_server_message(&abandoned.session_id, json!({ "large": "replay" }));

    let deadline = std::time::Instant::now() + Duration::from_millis(250);
    while sessions.session_count() != 0 && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }

    assert_eq!(sessions.session_count(), 0);
}

#[test]
fn dropping_the_registry_stops_its_idle_reaper() {
    let sessions = ReliableSessionRegistry::with_idle_timeout("server-1", Duration::from_secs(30));
    let stopped = sessions.reaper_stopped();
    assert!(!stopped.load(std::sync::atomic::Ordering::Acquire));

    drop(sessions);

    assert!(stopped.load(std::sync::atomic::Ordering::Acquire));
}

#[test]
fn repeated_lost_close_replacements_keep_the_server_registry_bounded() {
    let sessions = ReliableSessionRegistry::new("server-1");
    let first = sessions.open(ConnectionId::new("generation-0"));
    let mut last = first.clone();
    for generation in 1..=MAX_ACTIVE_RELIABLE_SESSIONS + 10 {
        last = sessions.open(ConnectionId::new(format!("generation-{generation}")));
    }

    assert_eq!(sessions.session_count(), MAX_ACTIVE_RELIABLE_SESSIONS);
    assert!(sessions.poll(&last.session_id, 0).is_ok());
}

#[test]
fn duplicate_client_frame_is_acknowledged_without_dispatching_twice() {
    let sessions = ReliableSessionRegistry::new("server-1");
    let opened = sessions.open(ConnectionId::new("local-http:client-1"));
    let mut dispatches = 0;

    let first = sessions.accept_client_frame(
        &opened.session_id,
        &ConnectionId::new("local-http:client-1"),
        1,
        json!({"jsonrpc": "2.0", "id": "request-1", "method": "task/list"}),
        |_| dispatches += 1,
    );
    let duplicate = sessions.accept_client_frame(
        &opened.session_id,
        &ConnectionId::new("local-http:client-1"),
        1,
        json!({"jsonrpc": "2.0", "id": "request-1", "method": "task/list"}),
        |_| dispatches += 1,
    );

    assert_eq!(first, AcceptClientFrame::Accepted);
    assert_eq!(duplicate, AcceptClientFrame::Duplicate);
    assert_eq!(dispatches, 1);
}

#[test]
fn server_frames_replay_until_the_client_acknowledges_their_sequence() {
    let sessions = ReliableSessionRegistry::new("server-1");
    let opened = sessions.open(ConnectionId::new("local-http:client-1"));
    sessions.enqueue_server_message(
        &opened.session_id,
        json!({"jsonrpc": "2.0", "method": "app/event", "params": {"cursor": "2"}}),
    );

    let first = sessions.poll(&opened.session_id, 0).unwrap();
    let replay = sessions.poll(&opened.session_id, 0).unwrap();
    let acknowledged = sessions.poll(&opened.session_id, 1).unwrap();

    assert_eq!(first, replay);
    assert_eq!(first.frames.len(), 1);
    assert_eq!(first.frames[0].sequence, 1);
    assert_eq!(acknowledged.frames, Vec::new());
}

#[test]
fn replay_window_expires_explicitly_instead_of_growing_without_bound() {
    let sessions = ReliableSessionRegistry::new("server-1");
    let opened = sessions.open(ConnectionId::new("local-http:client-1"));
    for value in 0..=MAX_SERVER_REPLAY_FRAMES {
        sessions.enqueue_server_message(&opened.session_id, json!({ "value": value }));
    }

    assert_eq!(
        sessions.poll(&opened.session_id, 0),
        Err(PollError::ReplayExpired)
    );
    let retained = sessions.poll(&opened.session_id, 1).unwrap();
    assert_eq!(retained.frames.len(), MAX_SERVER_REPLAY_FRAMES);
    assert_eq!(retained.frames[0].sequence, 2);
}
