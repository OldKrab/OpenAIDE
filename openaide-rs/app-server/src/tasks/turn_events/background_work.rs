use crate::agent::events::AgentBackgroundWork;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{BackgroundCommandOutcome, TaskBackgroundCommand, TaskBackgroundWork};
use crate::tasks::mutation::{TaskCommitOptions, TaskMutationResult};

use super::TaskSessionEventSink;

impl TaskSessionEventSink {
    /// Records the active turn's Background Work: its live commands and whether
    /// they alone hold the turn open. Entering that hold marks the Agent's
    /// finished output unread; the `finished` Task Attention Event still waits
    /// for the turn to settle.
    pub(super) fn update_background_work(
        &self,
        work: AgentBackgroundWork,
        now: &str,
    ) -> Result<(), RuntimeError> {
        let mut transition = None;
        self.mutations.commit_existing_task(
            &self.task_id,
            TaskCommitOptions::metadata(),
            |ctx| {
                let task = ctx.task();
                if task.agent_session_id.as_deref() != Some(self.session_id.as_str())
                    || task.active_turn_id.is_none()
                {
                    return Ok(TaskMutationResult::Unchanged);
                }
                let previous = task.background_work.as_ref();
                let next = (work.held || !work.commands.is_empty()).then(|| TaskBackgroundWork {
                    held: work.held,
                    commands: work
                        .commands
                        .iter()
                        .map(|command| {
                            let known = previous.and_then(|previous| {
                                previous
                                    .commands
                                    .iter()
                                    .find(|known| known.command_id == command.command_id)
                            });
                            TaskBackgroundCommand {
                                command_id: command.command_id.clone(),
                                description: command.description.clone(),
                                kind_label: command.kind_label.clone(),
                                // The first report starts the clock; later reports keep
                                // it, together with App Server's own stop result.
                                started_at: known.map_or_else(
                                    || now.to_string(),
                                    |known| known.started_at.clone(),
                                ),
                                paused: command.paused,
                                can_stop: command.can_stop,
                                stop_failed: known.is_some_and(|known| known.stop_failed),
                                tool_call_id: command.tool_call_id.clone(),
                            }
                        })
                        .collect(),
                });
                if previous == next.as_ref() {
                    return Ok(TaskMutationResult::Unchanged);
                }
                let was_held = previous.is_some_and(|previous| previous.held);
                let live_commands = work.commands.len();
                let task = ctx.task_mut();
                task.background_work = next;
                if work.held && !was_held {
                    task.unread = true;
                }
                task.updated_at = now.to_string();
                transition = Some((was_held, live_commands));
                Ok(TaskMutationResult::Changed)
            },
        )?;
        if let Some((was_held, live_commands)) = transition {
            crate::logging::info(
                "task_background_work_changed",
                serde_json::json!({
                    "task_id": self.task_id,
                    "session_id": self.session_id,
                    "phase": match (was_held, work.held) {
                        (false, true) => "entered",
                        (true, false) => "left",
                        _ => "updated",
                    },
                    "held": work.held,
                    "live_commands": live_commands,
                }),
            );
        }
        Ok(())
    }

    /// Marks the Tool row that started a background command with how the
    /// command ended. The row is history, so this needs no active turn.
    pub(super) fn record_background_command_outcome(
        &self,
        tool_call_id: &str,
        outcome: BackgroundCommandOutcome,
        now: &str,
    ) -> Result<(), RuntimeError> {
        let mut recorded = false;
        self.mutations.commit_existing_task(
            &self.task_id,
            TaskCommitOptions {
                refresh_message_history: true,
                response_snapshot_tail_limit: None,
            },
            |ctx| {
                if ctx.task().agent_session_id.as_deref() != Some(self.session_id.as_str())
                    || !ctx.record_tool_background_outcome(tool_call_id, outcome)
                {
                    return Ok(TaskMutationResult::Unchanged);
                }
                ctx.task_mut().updated_at = now.to_string();
                recorded = true;
                Ok(TaskMutationResult::Changed)
            },
        )?;
        crate::logging::info(
            "task_background_command_finished",
            serde_json::json!({
                "task_id": self.task_id,
                "session_id": self.session_id,
                "outcome": match outcome {
                    BackgroundCommandOutcome::Completed => "completed",
                    BackgroundCommandOutcome::Failed => "failed",
                    BackgroundCommandOutcome::Stopped => "stopped",
                },
                "tool_row_marked": recorded,
            }),
        );
        Ok(())
    }
}
