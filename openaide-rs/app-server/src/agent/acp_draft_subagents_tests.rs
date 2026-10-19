use super::*;

fn message(update: serde_json::Value) -> UntypedMessage {
    UntypedMessage::new(
        "session/update",
        serde_json::json!({ "sessionId": "root", "update": update }),
    )
    .unwrap()
}

#[test]
fn reads_both_draft_updates() {
    let spawned = decode_draft_subagent_update(&message(serde_json::json!({
        "sessionUpdate": "subagent_spawned",
        "subagentSessionId": "child",
        "name": "Researcher",
        "task": "Delegated task for Researcher",
        "capabilities": { "cancel": true },
        "prompt": 7,
        "_meta": "not an object",
    })))
    .unwrap()
    .unwrap();
    assert_eq!(spawned.session_id, "root");
    assert_eq!(spawned.announced_child(), Some("child"));
    let DraftSubagentUpdate::Spawned(spawned) = spawned.update else {
        panic!("expected a spawn");
    };
    assert!(spawned.capabilities.cancel && !spawned.capabilities.close);
    assert_eq!(
        spawned.prompt(),
        None,
        "a malformed prompt is dropped alone"
    );

    let finished = decode_draft_subagent_update(&message(serde_json::json!({
        "sessionUpdate": "subagent_state_update",
        "subagentSessionId": "child",
        "state": "paused",
    })))
    .unwrap()
    .unwrap();
    assert!(matches!(
        finished.update,
        DraftSubagentUpdate::StateChanged(DraftSubagentStateUpdate {
            state: DraftSubagentState::Unrecognized,
            ..
        })
    ));
}

#[test]
fn leaves_other_updates_to_typed_decoding() {
    assert!(decode_draft_subagent_update(&message(serde_json::json!({
        "sessionUpdate": "subagent_update",
        "sessionId": "child",
    })))
    .is_none());
}

#[test]
fn rejects_a_draft_update_without_its_child() {
    assert!(decode_draft_subagent_update(&message(serde_json::json!({
        "sessionUpdate": "subagent_state_update",
        "state": "completed",
    })))
    .unwrap()
    .is_err());
}
