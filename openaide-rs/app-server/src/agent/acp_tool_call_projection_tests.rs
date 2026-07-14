use crate::agent::acp_schema::{ToolCall, ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields};

use super::{
    merge_tool_call_update_with_status_change, remember_tool_call_with_status_change, ToolCallState,
};

#[test]
fn reports_only_actual_tool_status_transitions() {
    let calls = ToolCallState::default();

    assert!(remember_tool_call_with_status_change(
        &calls,
        ToolCall::new("tool_1", "Run checks").status(ToolCallStatus::InProgress),
    ));
    assert!(
        !merge_tool_call_update_with_status_change(
            &calls,
            ToolCallUpdate::new(
                "tool_1",
                ToolCallUpdateFields::new().status(ToolCallStatus::InProgress),
            ),
        )
        .1
    );
    assert!(
        merge_tool_call_update_with_status_change(
            &calls,
            ToolCallUpdate::new(
                "tool_1",
                ToolCallUpdateFields::new().status(ToolCallStatus::Completed),
            ),
        )
        .1
    );
}
