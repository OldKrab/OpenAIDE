use super::*;
use crate::agent::acp_schema::{
    CompactionStatus as AcpCompactionStatus, CompactionSummaryChunk, CompactionUpdate,
    MaybeUndefined,
};
use crate::agent::events::AgentCompactionChange;
use crate::protocol::model::CompactionStatus;

fn text(text: &str) -> ContentBlock {
    ContentBlock::Text(TextContent::new(text))
}

fn compaction_rows(messages: &[NormalizedMessage]) -> Vec<&NormalizedMessage> {
    messages
        .iter()
        .filter(|message| matches!(message, NormalizedMessage::Compaction { .. }))
        .collect()
}

#[test]
fn replayed_compaction_is_one_row_at_its_timeline_position_and_never_a_user_message() {
    let completed = CompactionUpdate::new("cmp_1", AcpCompactionStatus::Completed)
        .summary(vec![text("## Retained context\n\nThe user asked for X.")]);
    let messages = ReplayProjection::new("session-cmp").project(vec![
        SessionUpdate::AgentMessageChunk(
            ContentChunk::new(text("Before compaction")).message_id("m1"),
        ),
        SessionUpdate::CompactionUpdate(CompactionUpdate::new(
            "cmp_1",
            AcpCompactionStatus::InProgress,
        )),
        SessionUpdate::CompactionSummaryChunk(CompactionSummaryChunk::new(
            "cmp_1",
            text("## Retained context\n\n"),
        )),
        SessionUpdate::CompactionUpdate(completed),
        SessionUpdate::AgentMessageChunk(
            ContentChunk::new(text("After compaction")).message_id("m2"),
        ),
    ]);

    assert_eq!(messages.len(), 3);
    assert!(matches!(
        &messages[1],
        NormalizedMessage::Compaction { id, status: CompactionStatus::Completed, summary: Some(summary), error: None, .. }
            if id == "acp:session-cmp:compaction:cmp_1"
                && summary == "## Retained context\n\nThe user asked for X."
    ));
    assert!(!messages
        .iter()
        .any(|message| matches!(message, NormalizedMessage::User { .. })));
}

#[test]
fn replayed_terminal_first_compaction_and_streamed_summary_accumulate() {
    let messages = ReplayProjection::new("session-cmp").project(vec![
        SessionUpdate::CompactionUpdate(CompactionUpdate::new(
            "cmp_streamed",
            AcpCompactionStatus::InProgress,
        )),
        SessionUpdate::CompactionSummaryChunk(CompactionSummaryChunk::new(
            "cmp_streamed",
            text("First "),
        )),
        SessionUpdate::CompactionSummaryChunk(CompactionSummaryChunk::new(
            "cmp_streamed",
            text("second"),
        )),
        // The terminal update omits `summary`, so the streamed text is retained.
        SessionUpdate::CompactionUpdate(CompactionUpdate::new(
            "cmp_streamed",
            AcpCompactionStatus::Completed,
        )),
        SessionUpdate::CompactionUpdate(
            CompactionUpdate::new("cmp_other", AcpCompactionStatus::Failed)
                .error("context too small"),
        ),
    ]);

    let rows = compaction_rows(&messages);
    assert_eq!(rows.len(), 2);
    assert!(matches!(
        rows[0],
        NormalizedMessage::Compaction { status: CompactionStatus::Completed, summary: Some(summary), .. }
            if summary == "First second"
    ));
    assert!(matches!(
        rows[1],
        NormalizedMessage::Compaction { status: CompactionStatus::Failed, summary: None, error: Some(error), .. }
            if error == "context too small"
    ));
}

#[test]
fn compaction_summary_patch_distinguishes_omitted_cleared_and_replaced() {
    let update = |summary: MaybeUndefined<Vec<ContentBlock>>| {
        let mut update = CompactionUpdate::new("cmp_1", AcpCompactionStatus::Completed);
        update.summary = summary;
        SessionUpdate::CompactionUpdate(update)
    };
    let summary_after = |updates: Vec<SessionUpdate>| {
        let messages = ReplayProjection::new("session-cmp").project(updates);
        match &messages[0] {
            NormalizedMessage::Compaction { summary, .. } => summary.clone(),
            other => panic!("expected compaction, got {other:?}"),
        }
    };
    let first = update(MaybeUndefined::Value(vec![text("kept")]));

    assert_eq!(
        summary_after(vec![first.clone(), update(MaybeUndefined::Undefined)]),
        Some("kept".to_string())
    );
    assert_eq!(
        summary_after(vec![first.clone(), update(MaybeUndefined::Null)]),
        None
    );
    assert_eq!(
        summary_after(vec![first.clone(), update(MaybeUndefined::Value(vec![]))]),
        None
    );
    assert_eq!(
        summary_after(vec![
            first,
            update(MaybeUndefined::Value(vec![text("new")]))
        ]),
        Some("new".to_string())
    );
}

#[test]
fn unknown_compaction_status_is_preserved_without_inferring_a_lifecycle() {
    let messages =
        ReplayProjection::new("session-cmp").project(vec![SessionUpdate::CompactionUpdate(
            CompactionUpdate::new(
                "cmp_1",
                AcpCompactionStatus::Other("_vendor_phase".to_string()),
            ),
        )]);

    assert!(matches!(
        &messages[0],
        NormalizedMessage::Compaction {
            status: CompactionStatus::Unknown,
            ..
        }
    ));
}

#[test]
fn live_compaction_updates_emit_ordered_events_keyed_by_compaction_id() {
    let capture = Arc::new(CapturingEventSink::default());
    let sink: Arc<dyn AgentEventSink> = capture.clone();
    let projection =
        LivePromptProjection::new("claude-acp", sink, crate::agent::TurnCancellation::new());

    projection
        .emit(SessionUpdate::CompactionUpdate(CompactionUpdate::new(
            "cmp_1",
            AcpCompactionStatus::InProgress,
        )))
        .unwrap();
    projection
        .emit(SessionUpdate::CompactionSummaryChunk(
            CompactionSummaryChunk::new("cmp_1", text("Summary")),
        ))
        .unwrap();

    let events = capture.events();
    assert_eq!(events.len(), 2);
    assert!(matches!(
        &events[0],
        AgentEvent::Compaction {
            compaction_id,
            change: AgentCompactionChange::Update { status: CompactionStatus::InProgress, .. },
        } if compaction_id == "cmp_1"
    ));
    assert!(matches!(
        &events[1],
        AgentEvent::Compaction {
            compaction_id,
            change: AgentCompactionChange::SummaryChunk { text },
        } if compaction_id == "cmp_1" && text == "Summary"
    ));
}
