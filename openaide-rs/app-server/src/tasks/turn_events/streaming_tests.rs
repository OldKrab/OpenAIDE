use super::StreamingRuns;

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
        overflow[0]
            .previous
            .as_ref()
            .and_then(|run| run.source_message_id.as_deref()),
        Some("message-1")
    );
    assert_eq!(runs.finish_text("2").len(), 32);
    runs.agent_text_chunk(
        "next turn".to_string(),
        Some("message-next".to_string()),
        "3",
    )
    .expect("finalization releases all active stream slots");
}
