//! Agent-reported work that outlives a `session/prompt` response.
//!
//! The Claude adapter lets a backgrounded command run on after the prompt
//! returns, and the model answers the command's completion in a cycle no prompt
//! started. The adapter reports both: async task updates name the live
//! commands, and `_session/turn_ended` ends such a cycle. The prompt runner
//! keeps the Task turn open while this state says work remains, so one prompt
//! owner still decides when the Task becomes idle.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agent_client_protocol::{
    Agent, ConnectionTo, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse, UntypedMessage,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::agent::acp_schema::{SessionId, SessionUpdate};
use crate::agent::acp_trace::AcpTraceSession;
use crate::agent::events::{AgentBackgroundCommand, AgentBackgroundWork};
use crate::logging;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::BackgroundCommandOutcome;

#[cfg(test)]
#[path = "acp_background_work_tests.rs"]
mod tests;

/// Client capability `_meta` key that asks the adapter for async task updates.
pub(super) const ASYNC_TASKS_CAPABILITY: &str = "async-tasks";

const ASYNC_TASK_STATE_METHOD: &str = "_openaide/async_task_state";
const ASYNC_TASK_STOP_METHOD: &str = "_session/async_task/stop";

/// How long a finished background command may take to wake the model. The
/// adapter reports no cycle start, so the first update of the cycle is the only
/// evidence that one began.
const DEFAULT_FOLLOWUP_GRACE: Duration = Duration::from_secs(60);
const STOP_TIMEOUT: Duration = Duration::from_secs(5);

/// What the connection does with one raw inbound notification.
pub(super) enum AsyncTaskRouting {
    NotAsyncTask,
    /// A normalized state change, addressed to the session worker.
    Forward(UntypedMessage),
    /// An async task update that changes nothing tracked, or cannot be read.
    Drop,
}

/// Async task updates are not ACP `sessionUpdate` variants, so typed decoding
/// rejects them. Rewrite the tracked changes into a notification the session
/// worker reads in arrival order with every other update of its session.
pub(super) fn route_async_task_update(notification: &UntypedMessage) -> AsyncTaskRouting {
    if notification.method() != "session/update" {
        return AsyncTaskRouting::NotAsyncTask;
    }
    let params = notification.params();
    let Some(update) = params.get("update") else {
        return AsyncTaskRouting::NotAsyncTask;
    };
    let kind = update
        .get("sessionUpdate")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let text = |field: &str| {
        update
            .get(field)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    let tool_call_id = text("toolCallId");
    // `None` leaves liveness alone: a progress update only refines metadata.
    let state = match kind {
        "async_task_spawned" => Some(AsyncTaskState::Running),
        "async_task_state_update" => {
            let state = update
                .get("state")
                .and_then(|state| serde_json::from_value(state.clone()).ok());
            if state.is_none() {
                logging::warn(
                    "acp_async_task_update_ignored",
                    json!({ "update": kind, "reason": "malformed" }),
                );
                return AsyncTaskRouting::Drop;
            }
            state
        }
        "async_task_progress" if tool_call_id.is_some() || text("description").is_some() => None,
        "async_task_progress" => return AsyncTaskRouting::Drop,
        _ => return AsyncTaskRouting::NotAsyncTask,
    };
    let session_id = params.get("sessionId").and_then(serde_json::Value::as_str);
    let (Some(session_id), Some(async_task_id)) = (session_id, text("asyncTaskId")) else {
        logging::warn(
            "acp_async_task_update_ignored",
            json!({ "session_id": session_id, "update": kind, "reason": "malformed" }),
        );
        return AsyncTaskRouting::Drop;
    };
    let spawned = kind == "async_task_spawned";
    let forwarded = UntypedMessage::new(
        ASYNC_TASK_STATE_METHOD,
        AsyncTaskStateNotification {
            session_id: session_id.to_string().into(),
            async_task_id,
            state,
            description: text("description").or_else(|| spawned.then(|| text("name")).flatten()),
            // A shell command is the unlabeled default.
            kind_label: text("taskType").filter(|kind| kind != "shell"),
            tool_call_id,
            can_stop: update.get("canStop").and_then(serde_json::Value::as_bool),
        },
    );
    forwarded.map_or(AsyncTaskRouting::Drop, AsyncTaskRouting::Forward)
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonRpcNotification)]
#[notification(method = "_openaide/async_task_state")]
#[serde(rename_all = "camelCase")]
pub(super) struct AsyncTaskStateNotification {
    session_id: SessionId,
    pub(super) async_task_id: String,
    /// Absent on a metadata-only refinement of a known command.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) state: Option<AsyncTaskState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) kind_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) can_stop: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum AsyncTaskState {
    Running,
    Paused,
    Completed,
    Failed,
    Stopped,
}

impl AsyncTaskState {
    fn label(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
        }
    }
}

/// The adapter's announcement that a cycle no prompt started is over.
#[derive(Debug, Clone, Deserialize, Serialize, JsonRpcNotification)]
#[notification(method = "_session/turn_ended")]
#[serde(rename_all = "camelCase")]
pub(super) struct TurnEndedNotification {
    session_id: SessionId,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonRpcRequest)]
#[request(method = "_session/async_task/stop", response = AsyncTaskStopResponse)]
#[serde(rename_all = "camelCase")]
struct AsyncTaskStopRequest {
    session_id: SessionId,
    async_task_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonRpcResponse)]
struct AsyncTaskStopResponse {
    #[serde(default)]
    stopped: bool,
}

/// One Native Session's background work. The session worker owns it; clones
/// share the state with the update dispatch closures.
#[derive(Clone)]
pub(crate) struct BackgroundWork {
    state: Arc<Mutex<State>>,
    followup_grace: Duration,
}

/// A background command ended; reported once to the Task's Tool row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FinishedCommand {
    pub(super) tool_call_id: String,
    pub(super) outcome: BackgroundCommandOutcome,
}

#[derive(Default)]
struct State {
    /// Running or paused commands in spawn order.
    live_tasks: Vec<AgentBackgroundCommand>,
    /// Ended commands whose Tool row has not been told yet.
    finished: Vec<FinishedCommand>,
    /// Set while a settled prompt response waits for this work to end.
    hold_started: Option<Instant>,
    /// A command finished during the hold; its cycle has not shown itself yet.
    followup_deadline: Option<Instant>,
    /// Updates arrived during the hold and no `_session/turn_ended` followed.
    cycle_running: bool,
    /// Stops the Agent confirmed during the hold whose acknowledgement line has
    /// not arrived. The adapter writes it as agent text outside any cycle.
    stop_acknowledgements_due: usize,
}

impl Default for BackgroundWork {
    fn default() -> Self {
        Self::with_followup_grace(DEFAULT_FOLLOWUP_GRACE)
    }
}

impl BackgroundWork {
    pub(crate) fn with_followup_grace(followup_grace: Duration) -> Self {
        Self {
            state: Arc::default(),
            followup_grace,
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .expect("ACP background work lock poisoned")
    }

    pub(super) fn task_state_changed(&self, update: &AsyncTaskStateNotification) {
        let mut state = self.state();
        let position = state
            .live_tasks
            .iter()
            .position(|task| task.command_id == update.async_task_id);
        let outcome = match update.state {
            Some(AsyncTaskState::Completed) => Some(BackgroundCommandOutcome::Completed),
            Some(AsyncTaskState::Failed) => Some(BackgroundCommandOutcome::Failed),
            Some(AsyncTaskState::Stopped) => Some(BackgroundCommandOutcome::Stopped),
            Some(AsyncTaskState::Running | AsyncTaskState::Paused) | None => None,
        };
        let Some(outcome) = outcome else {
            let paused = update.state.map(|state| state == AsyncTaskState::Paused);
            match position {
                Some(position) => {
                    let task = &mut state.live_tasks[position];
                    if let Some(description) = &update.description {
                        task.description.clone_from(description);
                    }
                    if update.tool_call_id.is_some() {
                        task.tool_call_id.clone_from(&update.tool_call_id);
                    }
                    if let Some(paused) = paused {
                        task.paused = paused;
                    }
                }
                // A refinement of a command that already ended tracks nothing.
                None if update.state.is_none() => return,
                None => state.live_tasks.push(AgentBackgroundCommand {
                    command_id: update.async_task_id.clone(),
                    description: update.description.clone().unwrap_or_default(),
                    kind_label: update.kind_label.clone(),
                    paused: paused.unwrap_or(false),
                    can_stop: update.can_stop.unwrap_or(false),
                    tool_call_id: update.tool_call_id.clone(),
                }),
            }
            if let Some(reported) = update.state {
                logging::info(
                    "acp_async_task_state_changed",
                    json!({
                        "session_id": update.session_id.to_string(),
                        "state": reported.label(),
                        "live_tasks": state.live_tasks.len(),
                    }),
                );
            }
            return;
        };
        let Some(position) = position else {
            return;
        };
        let ended = state.live_tasks.remove(position);
        // The id can arrive with the terminal update alone.
        if let Some(tool_call_id) = update.tool_call_id.clone().or(ended.tool_call_id) {
            state.finished.push(FinishedCommand {
                tool_call_id,
                outcome,
            });
        }
        // The SDK tells the model about a finished command, not about one the
        // user stopped, so only the former promises a cycle.
        if state.hold_started.is_some() && outcome != BackgroundCommandOutcome::Stopped {
            state.followup_deadline = Some(Instant::now() + self.followup_grace);
        }
        logging::info(
            "acp_async_task_state_changed",
            json!({
                "session_id": update.session_id.to_string(),
                "state": update.state.map(AsyncTaskState::label),
                "live_tasks": state.live_tasks.len(),
            }),
        );
    }

    /// Any Chat-visible update during the hold belongs to a cycle the model
    /// started on its own: the prompt's own updates precede its response.
    /// The acknowledgement of a stop App Server asked for is the exception: no
    /// `_session/turn_ended` follows it.
    // TODO: the adapter also writes its mode and Fast mode fallback lines as
    // untagged agent text, which this still counts as a cycle nothing ends.
    // Tell them apart once the adapter tags them or App Server takes its
    // `notice` updates, and drop the acknowledgement count with them.
    pub(super) fn session_update_observed(&self, session_id: &str, update: &SessionUpdate) {
        let kind = match update {
            SessionUpdate::AgentMessageChunk(_) => "agent_message_chunk",
            SessionUpdate::AgentThoughtChunk(_) => "agent_thought_chunk",
            SessionUpdate::ToolCall(_) => "tool_call",
            SessionUpdate::ToolCallUpdate(_) => "tool_call_update",
            SessionUpdate::Plan(_) => "plan",
            _ => return,
        };
        let mut state = self.state();
        if state.hold_started.is_none() {
            return;
        }
        if kind == "agent_message_chunk" && state.stop_acknowledgements_due > 0 {
            state.stop_acknowledgements_due -= 1;
            return;
        }
        state.followup_deadline = None;
        if !std::mem::replace(&mut state.cycle_running, true) {
            logging::info(
                "acp_autonomous_cycle_observed",
                json!({
                    "session_id": session_id,
                    "first_update": kind,
                    "live_tasks": state.live_tasks.len(),
                }),
            );
        }
    }

    pub(super) fn turn_ended(&self, ended: &TurnEndedNotification) {
        let mut state = self.state();
        state.followup_deadline = None;
        let was_running = std::mem::take(&mut state.cycle_running);
        logging::info(
            "acp_autonomous_turn_ended",
            json!({
                "session_id": ended.session_id.to_string(),
                "held": state.hold_started.is_some(),
                "cycle_observed": was_running,
            }),
        );
    }

    /// A new prompt took over completion ownership, which ends the hold: its
    /// updates are its own output, not a cycle, and its response decides anew
    /// whether background work holds the turn.
    pub(super) fn prompt_continued(&self, session_id: &str, task_id: &str) {
        self.finish_hold(session_id, task_id, "prompt_continued");
    }

    /// Whether a settled `end_turn` must keep its turn open. The first `true`
    /// starts the hold; `finish_hold` ends it.
    pub(super) fn holds_turn(&self, session_id: &str, task_id: &str) -> bool {
        let mut state = self.state();
        let now = Instant::now();
        if state
            .followup_deadline
            .is_some_and(|deadline| deadline <= now)
        {
            state.followup_deadline = None;
            logging::warn(
                "acp_background_followup_timeout",
                json!({
                    "session_id": session_id,
                    "task_id": task_id,
                    "grace_ms": self.followup_grace.as_millis(),
                }),
            );
        }
        let holds = !state.live_tasks.is_empty()
            || state.cycle_running
            || state.followup_deadline.is_some();
        if holds && state.hold_started.is_none() {
            state.hold_started = Some(now);
            logging::info(
                "acp_background_hold_started",
                json!({
                    "session_id": session_id,
                    "task_id": task_id,
                    "live_tasks": state.live_tasks.len(),
                }),
            );
        }
        holds
    }

    pub(super) fn is_holding(&self) -> bool {
        self.state().hold_started.is_some()
    }

    /// What the Task shows of this work: the live commands, and whether they
    /// alone keep the answered turn open. `held` is false while the prompt or a
    /// cycle the Agent started is producing output.
    pub(super) fn report(&self, turn_held: bool) -> AgentBackgroundWork {
        let state = self.state();
        AgentBackgroundWork {
            held: turn_held && state.hold_started.is_some() && !state.cycle_running,
            commands: state.live_tasks.clone(),
        }
    }

    /// Ended commands not yet reported to their Tool rows.
    pub(super) fn take_finished(&self) -> Vec<FinishedCommand> {
        std::mem::take(&mut self.state().finished)
    }

    /// Resolves when the grace for an expected cycle ends; pending otherwise.
    pub(super) async fn followup_timeout(&self) {
        let deadline = self.state().followup_deadline;
        match deadline {
            Some(deadline) => tokio::time::sleep_until(deadline.into()).await,
            None => std::future::pending().await,
        }
    }

    /// The prompt runner is leaving, or a continuation prompt took over. Live
    /// commands stay tracked; the cycle flags describe this hold only.
    pub(super) fn finish_hold(&self, session_id: &str, task_id: &str, outcome: &'static str) {
        let mut state = self.state();
        state.cycle_running = false;
        state.followup_deadline = None;
        state.stop_acknowledgements_due = 0;
        let Some(started) = state.hold_started.take() else {
            return;
        };
        logging::info(
            "acp_background_hold_finished",
            json!({
                "session_id": session_id,
                "task_id": task_id,
                "outcome": outcome,
                "live_tasks": state.live_tasks.len(),
                "duration_ms": started.elapsed().as_millis(),
            }),
        );
    }

    /// User Stop ends the background commands with the turn. A command the
    /// Agent fails to stop keeps running; the next prompt still tracks it.
    pub(super) async fn stop_live_tasks(
        &self,
        connection: &ConnectionTo<Agent>,
        session_id: &SessionId,
        trace: Option<&AcpTraceSession>,
    ) {
        let tasks: Vec<String> = self
            .state()
            .live_tasks
            .iter()
            .map(|task| task.command_id.clone())
            .collect();
        for async_task_id in tasks {
            // Best effort: the turn ends whether or not a command stops.
            let _ = Self::stop_task(connection, session_id, trace, async_task_id).await;
        }
    }

    /// Stops one command for the user without touching the turn. The session
    /// worker awaits the answer before it reads the updates the stop caused, so
    /// the acknowledgement line is still ahead when the answer is counted.
    pub(super) async fn stop_command(
        &self,
        connection: &ConnectionTo<Agent>,
        session_id: &SessionId,
        trace: Option<&AcpTraceSession>,
        async_task_id: String,
    ) -> Result<(), RuntimeError> {
        let stopped = Self::stop_task(connection, session_id, trace, async_task_id).await?;
        if stopped {
            self.stop_confirmed();
        }
        Ok(())
    }

    /// The Agent stopped a command on request and acknowledges it in Chat.
    fn stop_confirmed(&self) {
        let mut state = self.state();
        if state.hold_started.is_some() {
            state.stop_acknowledgements_due += 1;
        }
    }

    /// Asks the Agent to stop one command. `Ok` means the Agent answered, with
    /// whether it stopped a running command; the command leaves the live set
    /// through its own state update, which may already have arrived.
    async fn stop_task(
        connection: &ConnectionTo<Agent>,
        session_id: &SessionId,
        trace: Option<&AcpTraceSession>,
        async_task_id: String,
    ) -> Result<bool, RuntimeError> {
        let started = Instant::now();
        logging::info(
            "acp_async_task_stop_started",
            json!({
                "operation": ASYNC_TASK_STOP_METHOD,
                "session_id": session_id.to_string(),
                "attempt": 1,
            }),
        );
        let request = AsyncTaskStopRequest {
            session_id: session_id.clone(),
            async_task_id,
        };
        if let Some(trace) = trace {
            trace.record(
                "client_to_agent",
                "_session/async_task/stop.request",
                &request,
            );
        }
        let result = tokio::time::timeout(
            STOP_TIMEOUT,
            connection.send_request_to(Agent, request).block_task(),
        )
        .await;
        let (outcome, error_code) = match &result {
            Ok(Ok(response)) if response.stopped => ("stopped", None),
            Ok(Ok(_)) => ("not_running", None),
            Ok(Err(error)) => ("error", Some(i32::from(error.code))),
            Err(_) => ("timeout", None),
        };
        logging::info(
            "acp_async_task_stop",
            json!({
                "operation": ASYNC_TASK_STOP_METHOD,
                "session_id": session_id.to_string(),
                "attempt": 1,
                "outcome": outcome,
                "error_code": error_code,
                "duration_ms": started.elapsed().as_millis(),
            }),
        );
        match result {
            Ok(Ok(response)) => Ok(response.stopped),
            Ok(Err(error)) => Err(RuntimeError::NotReady(format!(
                "Agent refused to stop the background command (code {})",
                i32::from(error.code)
            ))),
            Err(_) => Err(RuntimeError::OutcomeUnknown(
                "Stopping the background command timed out".to_string(),
            )),
        }
    }
}
