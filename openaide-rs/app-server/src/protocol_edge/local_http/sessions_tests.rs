use serde_json::json;

use crate::client_lifecycle::ConnectionId;

use super::*;

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
