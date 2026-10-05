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

#[test]
fn queued_server_frames_wake_a_waiting_link() {
    let signal = DeliverySignal::default();
    let sessions = ReliableSessionRegistry::with_delivery_signal("server-1", signal.clone());
    let opened = sessions.open(ConnectionId::new("local-http:client-1"));
    let seen = signal.generation();

    sessions.enqueue_server_message(&opened.session_id, json!({ "value": 1 }));

    assert_ne!(signal.generation(), seen);
}

#[test]
fn acknowledging_and_replaying_are_independent() {
    let sessions = ReliableSessionRegistry::new("server-1");
    let opened = sessions.open(ConnectionId::new("local-http:client-1"));
    for value in 1..=3 {
        sessions.enqueue_server_message(&opened.session_id, json!({ "value": value }));
    }

    // A link may have written every frame while the client applied only one.
    let unwritten = sessions.frames_after(&opened.session_id, 3).unwrap();
    sessions.acknowledge(&opened.session_id, 1).unwrap();
    let replay = sessions.frames_after(&opened.session_id, 1).unwrap();

    assert_eq!(unwritten, Vec::new());
    assert_eq!(
        replay
            .iter()
            .map(|frame| frame.sequence)
            .collect::<Vec<_>>(),
        [2, 3]
    );
    assert_eq!(
        sessions.frames_after(&opened.session_id, 0),
        Err(PollError::ReplayExpired)
    );
    assert_eq!(
        sessions.acknowledge(&opened.session_id, 4),
        Err(PollError::InvalidAcknowledgement)
    );
}

#[test]
fn a_newer_link_supersedes_the_previous_one_on_the_same_session() {
    let sessions = ReliableSessionRegistry::new("server-1");
    let connection_id = ConnectionId::new("local-http:client-1");
    let first = sessions.attach_link(connection_id.clone(), None).unwrap();
    sessions.accept_client_frame(&first.session_id, &connection_id, 1, json!({}), |_| {});

    let resumed = sessions
        .attach_link(connection_id.clone(), Some(&first.session_id))
        .unwrap();

    assert_eq!(resumed.session_id, first.session_id);
    assert_eq!(resumed.last_client_sequence, 1);
    assert!(sessions.link_is_current(&resumed));
    assert!(!sessions.link_is_current(&first));
}

#[test]
fn a_link_cannot_resume_a_missing_or_foreign_session() {
    let sessions = ReliableSessionRegistry::new("server-1");
    let owner = ConnectionId::new("local-http:client-1");
    let opened = sessions.open(owner);

    assert_eq!(
        sessions.attach_link(ConnectionId::new("local-http:client-2"), Some("missing")),
        Err(AttachError::UnknownSession)
    );
    assert_eq!(
        sessions.attach_link(
            ConnectionId::new("local-http:client-2"),
            Some(&opened.session_id)
        ),
        Err(AttachError::WrongConnection)
    );
}
