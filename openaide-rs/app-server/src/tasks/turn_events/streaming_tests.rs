use super::StreamingRuns;

#[test]
fn sourced_stream_limit_is_bounded_and_resets_after_turn_finalization() {
    let runs = StreamingRuns::default();
    for index in 0..256 {
        runs.agent_text_chunk("chunk".to_string(), Some(format!("message-{index}")), "1")
            .expect("stream within the active-turn limit");
    }

    let overflow = runs.agent_text_chunk(
        "overflow".to_string(),
        Some("message-overflow".to_string()),
        "1",
    );
    assert!(overflow.is_err());

    assert_eq!(runs.finish_text("2").len(), 256);
    runs.agent_text_chunk(
        "next turn".to_string(),
        Some("message-next".to_string()),
        "3",
    )
    .expect("finalization releases all active stream slots");
}
