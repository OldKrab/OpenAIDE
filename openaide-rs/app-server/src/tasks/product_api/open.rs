use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::snapshot::TaskSnapshot;
use openaide_app_server_protocol::task::TaskOpenParams;
use std::time::Instant;

use crate::agent::{
    AgentListSessionsRequest, AgentLoadedSession, AgentSessionLoad, TurnCancellation,
};
use crate::logging;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{AgentListedSession, TaskStatus as LegacyTaskStatus};
use crate::snapshots::task_snapshot::project_stored_task_snapshot;
use crate::storage::records::TaskRecord;
use crate::tasks::mutation::{TaskCommitOptions, TaskCommitOutcome, TaskMutationResult};
use crate::tasks::task_start_transaction::TaskSessionStartGuard;

use super::{internal_error, protocol_error_from_runtime, runtime_error, TaskProductApi};

pub(crate) trait TaskOpenWorkflow: Send + Sync {
    fn open(&self, params: TaskOpenParams) -> Result<TaskSnapshot, ProtocolError>;
}

impl TaskProductApi {
    pub(super) fn open_task(&self, params: TaskOpenParams) -> Result<TaskSnapshot, ProtocolError> {
        let task_id = params.task_id.as_str().to_string();
        let task = self.store.read_task(&task_id).map_err(runtime_error)?;
        super::reject_tombstoned_task(&task)?;

        if let Some(snapshot) = self.refresh_adopted_task_from_native_session_if_newer(&task)? {
            return project_stored_task_snapshot(snapshot);
        }

        let result = self
            .mutations
            .commit_existing_task(&task_id, super::response_snapshot_options(), |ctx| {
                if ctx.task().tombstoned {
                    return Err(RuntimeError::TaskNotFound(task_id.clone()));
                }
                if !ctx.task().unread {
                    return Ok(TaskMutationResult::Unchanged);
                }
                ctx.task_mut().unread = false;
                Ok(TaskMutationResult::Changed)
            })
            .map_err(protocol_error_from_runtime)?;
        let snapshot = result
            .response_snapshot
            .ok_or_else(|| internal_error("missing task open snapshot"))?;
        project_stored_task_snapshot(snapshot)
    }

    fn refresh_adopted_task_from_native_session_if_newer(
        &self,
        task: &TaskRecord,
    ) -> Result<Option<crate::protocol::model::TaskSnapshot>, ProtocolError> {
        if task.status == LegacyTaskStatus::Active || task.active_turn_id.is_some() {
            return Ok(None);
        }
        let Some(stored_session_id) = task.agent_session_id.clone() else {
            return Ok(None);
        };
        let Some(native_session) = self.newer_native_session_for_task(task, &stored_session_id)?
        else {
            return Ok(None);
        };
        let refresh_started = Instant::now();
        let load_started = Instant::now();
        let loaded = self.load_adopted_session_for_refresh(AgentSessionLoad {
            agent_id: task.agent_id.clone(),
            task_id: task.task_id.clone(),
            cwd: task.workspace_root.clone(),
            model_id: task.model_id.clone(),
            session_id: stored_session_id.clone(),
            cancellation: TurnCancellation::new(),
            secret_resolver: Some(self.task_secret_resolver(&task.task_id)),
        })?;
        let load_ms = load_started.elapsed().as_millis();
        let mut session_start =
            TaskSessionStartGuard::new(&self.agent_gateway, loaded.session.clone());
        let loaded_session_id = session_start.session_id().to_string();
        let refreshed_at = native_session
            .last_activity
            .clone()
            .unwrap_or_else(crate::time::now_string);
        let refreshed_title = native_session
            .title
            .as_deref()
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_string);
        let config_options = loaded.session.config_options.clone();
        let config_options_catalog = loaded.session.config_catalog.clone();
        let agent_commands_catalog = loaded.session.commands_catalog.clone();
        let model_id = loaded.session.model_id.clone();
        let replayed_messages = loaded.replayed_messages;
        let replayed_message_count = replayed_messages.len();

        let commit_started = Instant::now();
        let result = self
            .mutations
            .commit_existing_task(
                &task.task_id,
                TaskCommitOptions {
                    refresh_message_history: true,
                    response_snapshot_tail_limit: Some(100),
                },
                |ctx| {
                    if ctx.task().agent_session_id.as_deref() != Some(stored_session_id.as_str())
                        || ctx.task().status == LegacyTaskStatus::Active
                        || ctx.task().active_turn_id.is_some()
                    {
                        return Ok(TaskMutationResult::Unchanged);
                    }
                    ctx.replace_messages(replayed_messages)?;
                    let task = ctx.task_mut();
                    if let Some(title) = refreshed_title {
                        task.agent_title = Some(title);
                    }
                    task.status = LegacyTaskStatus::Inactive;
                    task.unread = false;
                    task.first_prompt_sent = true;
                    task.agent_session_id = Some(loaded_session_id.clone());
                    task.config_options = config_options;
                    task.config_options_catalog = config_options_catalog;
                    task.agent_commands_catalog = agent_commands_catalog;
                    task.model_id = model_id;
                    task.updated_at = refreshed_at.clone();
                    task.last_activity = refreshed_at;
                    Ok(TaskMutationResult::Changed)
                },
            )
            .map_err(protocol_error_from_runtime)?;
        let commit_ms = commit_started.elapsed().as_millis();

        let snapshot = match result.outcome {
            TaskCommitOutcome::Committed(_) => result
                .response_snapshot
                .ok_or_else(|| internal_error("missing refreshed task snapshot"))?,
            TaskCommitOutcome::Rejected(_) => {
                let _ = session_start.close();
                return Ok(None);
            }
        };

        let attach_started = Instant::now();
        if let Err(error) = self
            .turn_runner
            .attach_session_events(task.task_id.clone(), &loaded_session_id)
        {
            let _ = session_start.close();
            return Err(protocol_error_from_runtime(error));
        }
        let attach_ms = attach_started.elapsed().as_millis();
        let _session = session_start.commit();
        logging::info(
            "adopted_task_refresh_timing",
            serde_json::json!({
                "task_id": task.task_id,
                "agent_id": task.agent_id,
                "message_count": replayed_message_count,
                "load_ms": load_ms,
                "commit_ms": commit_ms,
                "attach_ms": attach_ms,
                "total_ms": refresh_started.elapsed().as_millis(),
            }),
        );
        Ok(Some(snapshot))
    }

    fn load_adopted_session_for_refresh(
        &self,
        request: AgentSessionLoad,
    ) -> Result<AgentLoadedSession, ProtocolError> {
        match self.agent_gateway.load_session(request.clone()) {
            Ok(loaded) => Ok(loaded),
            Err(RuntimeError::InvalidParams(message))
                if message == "agent_session_id already active" =>
            {
                self.agent_gateway
                    .close_session(&request.session_id)
                    .map_err(protocol_error_from_runtime)?;
                self.agent_gateway
                    .load_session(request)
                    .map_err(protocol_error_from_runtime)
            }
            Err(error) => Err(protocol_error_from_runtime(error)),
        }
    }

    fn newer_native_session_for_task(
        &self,
        task: &TaskRecord,
        session_id: &str,
    ) -> Result<Option<AgentListedSession>, ProtocolError> {
        let mut cursor = None;
        loop {
            let result = match self.agent_gateway.list_sessions(AgentListSessionsRequest {
                agent_id: task.agent_id.clone(),
                cwd: task.workspace_root.clone(),
                cursor: cursor.clone(),
            }) {
                Ok(result) => result,
                Err(_) => return Ok(None),
            };
            if let Some(session) = result
                .sessions
                .into_iter()
                .find(|session| session.session_id == session_id)
            {
                return Ok(native_session_is_newer(task, &session).then_some(session));
            }
            if result.next_cursor.is_none() || result.next_cursor == cursor {
                return Ok(None);
            }
            cursor = result.next_cursor;
        }
    }
}

impl TaskOpenWorkflow for TaskProductApi {
    fn open(&self, params: TaskOpenParams) -> Result<TaskSnapshot, ProtocolError> {
        self.open_task(params)
    }
}

fn native_session_is_newer(task: &TaskRecord, session: &AgentListedSession) -> bool {
    let Some(native_last_activity) = session.last_activity.as_deref() else {
        return false;
    };
    timestamp_compare(native_last_activity, &task.last_activity).is_gt()
}

fn timestamp_compare(left: &str, right: &str) -> std::cmp::Ordering {
    let left_millis = timestamp_millis(left);
    let right_millis = timestamp_millis(right);
    match (left_millis, right_millis) {
        (Some(left), Some(right)) => left.cmp(&right),
        _ => left.cmp(right),
    }
}

fn timestamp_millis(value: &str) -> Option<i128> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.as_bytes().iter().all(|byte| byte.is_ascii_digit()) {
        return trimmed.parse::<i128>().ok();
    }
    iso_utc_millis(trimmed)
}

fn iso_utc_millis(value: &str) -> Option<i128> {
    let bytes = value.as_bytes();
    if bytes.len() < 20
        || bytes.get(4) != Some(&b'-')
        || bytes.get(7) != Some(&b'-')
        || !matches!(bytes.get(10), Some(b'T' | b't'))
        || bytes.get(13) != Some(&b':')
        || bytes.get(16) != Some(&b':')
        || !value.ends_with('Z')
    {
        return None;
    }
    let year = parse_digits(value, 0, 4)? as i32;
    let month = parse_digits(value, 5, 7)? as u32;
    let day = parse_digits(value, 8, 10)? as u32;
    let hour = parse_digits(value, 11, 13)? as u32;
    let minute = parse_digits(value, 14, 16)? as u32;
    let second = parse_digits(value, 17, 19)? as u32;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let millis = if bytes.get(19) == Some(&b'.') {
        parse_fraction_millis(&value[20..value.len() - 1])?
    } else if bytes.get(19) == Some(&b'Z') {
        0
    } else {
        return None;
    };
    let days = days_from_civil(year, month, day)?;
    Some(
        (((days * 24 + hour as i128) * 60 + minute as i128) * 60 + second as i128) * 1000
            + millis as i128,
    )
}

fn parse_digits(value: &str, start: usize, end: usize) -> Option<u32> {
    value.get(start..end)?.parse().ok()
}

fn parse_fraction_millis(fraction: &str) -> Option<u32> {
    if fraction.is_empty() || !fraction.as_bytes().iter().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let mut digits = fraction
        .as_bytes()
        .iter()
        .take(3)
        .map(|digit| (digit - b'0') as u32);
    let hundreds = digits.next().unwrap_or(0);
    let tens = digits.next().unwrap_or(0);
    let ones = digits.next().unwrap_or(0);
    Some(hundreds * 100 + tens * 10 + ones)
}

fn days_from_civil(year: i32, month: u32, day: u32) -> Option<i128> {
    let leap = is_leap_year(year);
    let month_lengths = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if day == 0 || day > month_lengths[(month - 1) as usize] {
        return None;
    }
    let year = year - (month <= 2) as i32;
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month = month as i32;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day as i32 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146097 + doe - 719468) as i128)
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}
