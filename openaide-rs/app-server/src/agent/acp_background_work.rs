//! Agent-reported work that outlives a `session/prompt` response.
//!
//! The Claude adapter lets a backgrounded command run on after the prompt
//! returns, and the model answers the command's completion in a cycle no prompt
//! started. The adapter reports both: async task updates name the live
//! commands, and `_session/turn_ended` ends such a cycle. The prompt runner
//! keeps the Task turn open while this state says work remains, so one prompt
//! owner still decides when the Task becomes idle.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agent_client_protocol::{
    Agent, ConnectionTo, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse, UntypedMessage,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::agent::acp_schema::{SessionId, SessionUpdate};
use crate::agent::acp_trace::AcpTraceSession;
use crate::logging;

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
    /// An async task update that changes no liveness, or cannot be read.
    Drop,
}

/// Async task updates are not ACP `sessionUpdate` variants, so typed decoding
/// rejects them. Rewrite the liveness changes into a notification the session
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
    let state = match kind {
        "async_task_spawned" => Some(AsyncTaskState::Running),
        "async_task_state_update" => update
            .get("state")
            .and_then(|state| serde_json::from_value(state.clone()).ok()),
        "async_task_progress" => return AsyncTaskRouting::Drop,
        _ => return AsyncTaskRouting::NotAsyncTask,
    };
    let session_id = params.get("sessionId").and_then(serde_json::Value::as_str);
    let async_task_id = update
        .get("asyncTaskId")
        .and_then(serde_json::Value::as_str);
    let (Some(session_id), Some(async_task_id), Some(state)) = (session_id, async_task_id, state)
    else {
        logging::warn(
            "acp_async_task_update_ignored",
            json!({ "session_id": session_id, "update": kind, "reason": "malformed" }),
        );
        return AsyncTaskRouting::Drop;
    };
    let forwarded = UntypedMessage::new(
        ASYNC_TASK_STATE_METHOD,
        AsyncTaskStateNotification {
            session_id: session_id.to_string().into(),
            async_task_id: async_task_id.to_string(),
            state,
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
    pub(super) state: AsyncTaskState,
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

#[derive(Default)]
struct State {
    live_tasks: BTreeSet<String>,
    /// Set while a settled prompt response waits for this work to end.
    hold_started: Option<Instant>,
    /// A command finished during the hold; its cycle has not shown itself yet.
    followup_deadline: Option<Instant>,
    /// Updates arrived during the hold and no `_session/turn_ended` followed.
    cycle_running: bool,
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
        let changed = match update.state {
            AsyncTaskState::Running | AsyncTaskState::Paused => {
                state.live_tasks.insert(update.async_task_id.clone())
            }
            AsyncTaskState::Completed | AsyncTaskState::Failed | AsyncTaskState::Stopped => {
                state.live_tasks.remove(&update.async_task_id)
            }
        };
        if !changed {
            return;
        }
        // The SDK tells the model about a finished command, not about one the
        // user stopped, so only the former promises a cycle.
        if state.hold_started.is_some()
            && matches!(
                update.state,
                AsyncTaskState::Completed | AsyncTaskState::Failed
            )
        {
            state.followup_deadline = Some(Instant::now() + self.followup_grace);
        }
        logging::info(
            "acp_async_task_state_changed",
            json!({
                "session_id": update.session_id.to_string(),
                "state": update.state.label(),
                "live_tasks": state.live_tasks.len(),
            }),
        );
    }

    /// Any Chat-visible update during the hold belongs to a cycle the model
    /// started on its own: the prompt's own updates precede its response.
    pub(super) fn session_update_observed(&self, update: &SessionUpdate) {
        if !matches!(
            update,
            SessionUpdate::AgentMessageChunk(_)
                | SessionUpdate::AgentThoughtChunk(_)
                | SessionUpdate::ToolCall(_)
                | SessionUpdate::ToolCallUpdate(_)
                | SessionUpdate::Plan(_)
        ) {
            return;
        }
        let mut state = self.state();
        if state.hold_started.is_some() {
            state.cycle_running = true;
            state.followup_deadline = None;
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

    /// A new prompt took over completion ownership; its response ends its work.
    pub(super) fn prompt_continued(&self) {
        let mut state = self.state();
        state.cycle_running = false;
        state.followup_deadline = None;
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

    /// How the Task should show the hold: the live command count while only
    /// background work keeps the turn open, `None` once a cycle the Agent
    /// started is producing output in it.
    pub(super) fn background_only(&self) -> Option<u32> {
        let state = self.state();
        (state.hold_started.is_some() && !state.cycle_running)
            .then(|| u32::try_from(state.live_tasks.len()).unwrap_or(u32::MAX))
    }

    /// Resolves when the grace for an expected cycle ends; pending otherwise.
    pub(super) async fn followup_timeout(&self) {
        let deadline = self.state().followup_deadline;
        match deadline {
            Some(deadline) => tokio::time::sleep_until(deadline.into()).await,
            None => std::future::pending().await,
        }
    }

    /// The prompt runner is leaving. Live commands stay tracked for the next
    /// prompt; the cycle flags describe this hold only.
    pub(super) fn finish_hold(&self, session_id: &str, task_id: &str, outcome: &'static str) {
        let mut state = self.state();
        state.cycle_running = false;
        state.followup_deadline = None;
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
        let tasks: Vec<String> = self.state().live_tasks.iter().cloned().collect();
        for async_task_id in tasks {
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
        }
    }
}
