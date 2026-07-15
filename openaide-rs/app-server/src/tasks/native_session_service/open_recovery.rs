use super::*;
use crate::protocol::model::{AgentListedSession, TaskSnapshot};

pub(crate) struct HistoryRefreshRequest {
    pub(crate) task: TaskRecord,
    pub(crate) stored_session_id: String,
    pub(crate) native_session: AgentListedSession,
    pub(crate) native_updated_at: u128,
    pub(crate) refreshed_at: String,
}

pub(crate) enum OpenSessionResumeOutcome {
    Resumed,
    Unsupported,
}

impl NativeSessionService {
    /// Loads and replaces Chat only after Task opening requires a replay.
    pub(crate) fn refresh_history(
        &self,
        request: HistoryRefreshRequest,
    ) -> Result<Option<TaskSnapshot>, RuntimeError> {
        let HistoryRefreshRequest {
            task,
            stored_session_id,
            native_session,
            native_updated_at,
            refreshed_at,
        } = request;
        let current_task = self.mutations.store().read_task(&task.task_id)?;
        if current_task.agent_session_id.as_deref() != Some(stored_session_id.as_str())
            || matches!(
                current_task.status,
                TaskStatus::Starting | TaskStatus::Active
            )
            || current_task.active_turn_id.is_some()
        {
            return Ok(None);
        }
        let load_started = std::time::Instant::now();
        let loaded = self.agent_gateway.load_session(AgentSessionLoad {
            agent_id: task.agent_id.clone(),
            task_id: task.task_id.clone(),
            cwd: task.workspace_root.clone(),
            model_id: task.model_id.clone(),
            session_id: stored_session_id.clone(),
            cancellation: TurnCancellation::new(),
            secret_resolver: Some(self.secret_resolver(&task.task_id)),
        })?;
        let load_ms = load_started.elapsed().as_millis();
        let session_start = TaskSessionStartGuard::new(&self.agent_gateway, loaded.session);
        let loaded_session_id = session_start.session_id().to_string();
        let refreshed_title = native_session
            .title
            .as_deref()
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_string);
        let session_state = OpenedSessionTaskState {
            session: session_start.session().clone(),
            metadata_is_authoritative: true,
        };
        let replayed_messages = loaded.replayed_messages;
        let replayed_message_count = replayed_messages.len();

        let commit_started = std::time::Instant::now();
        let result = self.mutations.commit_existing_task(
            &task.task_id,
            TaskCommitOptions {
                refresh_message_history: true,
                response_snapshot_tail_limit: Some(100),
            },
            |ctx| {
                if ctx.task().agent_session_id.as_deref() != Some(stored_session_id.as_str())
                    || matches!(ctx.task().status, TaskStatus::Starting | TaskStatus::Active)
                    || ctx.task().active_turn_id.is_some()
                {
                    return Ok(TaskMutationResult::Unchanged);
                }
                ctx.replace_messages_from_native_session(replayed_messages, native_updated_at)?;
                session_state.apply_to(ctx.task_mut());
                let task = ctx.task_mut();
                if let Some(title) = refreshed_title {
                    task.set_agent_title(&title);
                }
                task.status = TaskStatus::Inactive;
                task.unread = false;
                task.agent_session_id = Some(loaded_session_id.clone());
                task.updated_at = refreshed_at.clone();
                task.last_activity = refreshed_at.clone();
                Ok(TaskMutationResult::Changed)
            },
        )?;
        let commit_ms = commit_started.elapsed().as_millis();
        let snapshot = match result.outcome {
            TaskCommitOutcome::Committed(_) => result.response_snapshot.ok_or_else(|| {
                RuntimeError::Internal("missing refreshed Task snapshot".to_string())
            })?,
            TaskCommitOutcome::Rejected(_) => return Ok(None),
        };

        let attach_started = std::time::Instant::now();
        self.ensure_update_subscription(&task.task_id, &session_start.session().key())?;
        let attach_ms = attach_started.elapsed().as_millis();
        session_start.commit();
        crate::logging::info(
            "native_session_history_refresh_timing",
            serde_json::json!({
                "task_id": task.task_id,
                "agent_id": task.agent_id,
                "message_count": replayed_message_count,
                "load_ms": load_ms,
                "commit_ms": commit_ms,
                "attach_ms": attach_ms,
            }),
        );
        Ok(Some(snapshot))
    }

    /// Reconnects an existing Task without replaying Chat and republishes any
    /// session metadata returned by the Agent before subscribing to live updates.
    pub(crate) fn resume_for_open(
        &self,
        task: &TaskRecord,
        stored_session_id: &str,
    ) -> Result<OpenSessionResumeOutcome, RuntimeError> {
        let current_task = self.mutations.store().read_task(&task.task_id)?;
        if current_task.agent_session_id.as_deref() != Some(stored_session_id)
            || matches!(
                current_task.status,
                TaskStatus::Starting | TaskStatus::Active
            )
            || current_task.active_turn_id.is_some()
        {
            return Ok(OpenSessionResumeOutcome::Resumed);
        }

        let resume_started = std::time::Instant::now();
        let session = match self.agent_gateway.resume_session(AgentSessionResume {
            agent_id: task.agent_id.clone(),
            task_id: task.task_id.clone(),
            session_id: stored_session_id.to_string(),
            cwd: task.workspace_root.clone(),
            model_id: task.model_id.clone(),
            cancellation: TurnCancellation::new(),
            secret_resolver: Some(self.secret_resolver(&task.task_id)),
        }) {
            Ok(session) => session,
            Err(RuntimeError::CapabilityMissing(_)) => {
                return Ok(OpenSessionResumeOutcome::Unsupported)
            }
            Err(error) => return Err(error),
        };
        let resume_ms = resume_started.elapsed().as_millis();
        let returned_config_catalog = session.config_catalog.is_some();
        let returned_commands_catalog = session.commands_catalog.is_some();
        let session_key = session.key();
        let session_state = OpenedSessionTaskState {
            session,
            metadata_is_authoritative: false,
        };
        let result = self.mutations.commit_existing_task(
            &task.task_id,
            TaskCommitOptions::metadata(),
            |ctx| {
                if ctx.task().agent_session_id.as_deref() != Some(stored_session_id)
                    || matches!(ctx.task().status, TaskStatus::Starting | TaskStatus::Active)
                    || ctx.task().active_turn_id.is_some()
                {
                    return Ok(TaskMutationResult::Rejected);
                }
                session_state.apply_to(ctx.task_mut());
                Ok(TaskMutationResult::Changed)
            },
        )?;
        if matches!(result.outcome, TaskCommitOutcome::Rejected(_)) {
            return Ok(OpenSessionResumeOutcome::Resumed);
        }
        self.ensure_update_subscription(&task.task_id, &session_key)?;
        crate::logging::info(
            "native_session_open_resume_timing",
            serde_json::json!({
                "task_id": task.task_id,
                "agent_id": task.agent_id,
                "resume_ms": resume_ms,
                "returned_config_catalog": returned_config_catalog,
                "returned_commands_catalog": returned_commands_catalog,
            }),
        );
        Ok(OpenSessionResumeOutcome::Resumed)
    }
}
