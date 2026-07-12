use super::StreamingRuns;

#[test]
fn first_sourced_chunk_starts_a_distinct_run_after_anonymous_text() {
    let runs = StreamingRuns::default();
    let anonymous = runs
        .agent_text_chunk("anonymous".to_string(), None, "1")
        .expect("anonymous chunk");
    let anonymous_id = message_id(&anonymous[0]);

    let sourced = runs
        .agent_text_chunk(
            "sourced".to_string(),
            Some("agent-message-1".to_string()),
            "2",
        )
        .expect("sourced chunk");

    assert_eq!(sourced.len(), 2);
    assert_eq!(message_id(&sourced[0]), anonymous_id);
    assert!(matches!(
        sourced[0].delta,
        super::StreamingDelta::Chunk(ref chunk) if chunk.final_chunk
    ));
    assert_ne!(message_id(&sourced[1]), anonymous_id);
    assert!(matches!(sourced[1].delta, super::StreamingDelta::Append));
}

#[test]
fn rolling_back_sourced_start_does_not_erase_the_anonymous_run() {
    let runs = StreamingRuns::default();
    let anonymous = runs
        .agent_text_chunk("anonymous".to_string(), None, "1")
        .expect("anonymous chunk");
    let anonymous_id = message_id(&anonymous[0]);
    let sourced = runs
        .agent_text_chunk(
            "sourced".to_string(),
            Some("agent-message-1".to_string()),
            "2",
        )
        .expect("sourced chunk");

    for write in sourced.into_iter().rev() {
        runs.rollback(write);
    }
    let resumed = runs
        .agent_text_chunk(" resumed".to_string(), None, "3")
        .expect("resumed anonymous chunk");

    assert_eq!(message_id(&resumed[0]), anonymous_id);
    assert!(matches!(resumed[0].delta, super::StreamingDelta::Chunk(_)));
}

#[test]
fn sourced_streams_finalize_the_oldest_message_instead_of_failing() {
    let runs = StreamingRuns::default();
    for index in 0..32 {
        runs.agent_text_chunk("chunk".to_string(), Some(format!("message-{index}")), "1")
            .expect("recent stream");
    }
    runs.agent_text_chunk(
        "newer chunk".to_string(),
        Some("message-0".to_string()),
        "1",
    )
    .expect("recent update");

    let overflow = runs
        .agent_text_chunk(
            "overflow".to_string(),
            Some("message-overflow".to_string()),
            "1",
        )
        .expect("old streams should be finalized to make room");

    assert_eq!(overflow.len(), 2);
    assert!(
        matches!(overflow[0].delta, super::StreamingDelta::Chunk(ref chunk) if chunk.final_chunk)
    );
    assert_eq!(
        match &overflow[0].run_key {
            super::RunKey::Sourced(source_message_id) => Some(source_message_id.as_str()),
            super::RunKey::Anonymous => None,
        },
        Some("message-1"),
    );
    assert_eq!(runs.finish_text("2").len(), 32);
    runs.agent_text_chunk(
        "next turn".to_string(),
        Some("message-next".to_string()),
        "3",
    )
    .expect("finalization releases all active stream slots");
}

fn message_id(write: &super::StreamingWrite) -> String {
    match &write.message {
        crate::protocol::model::NormalizedMessage::AgentText { id, .. }
        | crate::protocol::model::NormalizedMessage::Thought { id, .. } => id.clone(),
        message => panic!("unexpected streaming message: {message:?}"),
    }
}
