use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use openaide_app_server_protocol::ids::TurnId;

use crate::agent::registry_handle::AgentRegistryHandle;
use crate::agent::{
    AgentSession, AgentSessionKey, AgentSessionLoad, AgentSessionResume, AgentSessionStart,
    ConfigOptionPolicy, TurnCancellation,
};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{AgentListedSession, Attachment, TaskSnapshot, TaskStatus};
use crate::server_requests::ServerRequestRuntime;
use crate::storage::records::{TaskLifecycle, TaskPreparationRecord, TaskRecord};
use crate::tasks::mutation::{
    TaskCommitOptions, TaskCommitOutcome, TaskMutationResult, TaskMutations,
};
use crate::tasks::product_api::secret_resolver::task_secret_resolver;
use crate::tasks::task_start_transaction::TaskSessionStartGuard;
use crate::tasks::turn_events::TaskSessionEventSink;
use crate::tasks::turns::TurnRunner;
use crate::time::now_string;

use crate::agent::gateway::AgentGateway;

/// Owns Native Session acquisition, binding, update subscription, and prompt startup.
/// Product workflows provide intent; they do not choose ACP start/load/resume paths.
#[derive(Clone)]
pub(crate) struct NativeSessionService {
    agent_registry: AgentRegistryHandle,
    agent_gateway: AgentGateway,
    mutations: TaskMutations,
    turn_runner: TurnRunner,
    server_requests: ServerRequestRuntime,
    preparing_session_ids: Arc<Mutex<HashSet<AgentSessionKey>>>,
    subscriptions: Arc<Mutex<HashMap<String, NativeSessionSubscription>>>,
}

pub(crate) struct PrimaryPromptRequest {
    pub(crate) task: TaskRecord,
    pub(crate) turn_id: TurnId,
    pub(crate) text: String,
    pub(crate) attachments: Vec<Attachment>,
}

pub(crate) struct HistoryRefreshRequest {
    pub(crate) task: TaskRecord,
    pub(crate) stored_session_id: String,
    pub(crate) native_session: AgentListedSession,
    pub(crate) native_updated_at: u128,
    pub(crate) refreshed_at: String,
}

impl NativeSessionService {
    pub(crate) fn new(
        agent_registry: AgentRegistryHandle,
        agent_gateway: AgentGateway,
        mutations: TaskMutations,
        turn_runner: TurnRunner,
        server_requests: ServerRequestRuntime,
        preparing_session_ids: Arc<Mutex<HashSet<AgentSessionKey>>>,
    ) -> Self {
        Self {
            agent_registry,
            agent_gateway,
            mutations,
            turn_runner,
            server_requests,
            preparing_session_ids,
            subscriptions: Default::default(),
        }
    }

    /// Acquires and binds the empty New Task's Native Session before Composer becomes sendable.
    pub(crate) fn prepare_task(&self, task: &TaskRecord) -> Result<(), RuntimeError> {
        let cancellation = TurnCancellation::new();
        let start = || {
            self.agent_gateway.start_session(AgentSessionStart {
                agent_id: task.agent_id.clone(),
                task_id: task.task_id.clone(),
                cwd: task.workspace_root.clone(),
                model_id: task.model_id.clone(),
                config_options: config_options_payload(task),
                config_option_policy: ConfigOptionPolicy::ReconcileWithAgentDefaults,
                context: Vec::new(),
                cancellation: cancellation.clone(),
                secret_resolver: Some(self.secret_resolver(&task.task_id)),
            })
        };
        let session = match &task.agent_session_id {
            Some(session_id) => self
                .agent_gateway
                .resume_session(AgentSessionResume {
                    agent_id: task.agent_id.clone(),
                    task_id: task.task_id.clone(),
                    session_id: session_id.clone(),
                    cwd: task.workspace_root.clone(),
                    model_id: task.model_id.clone(),
                    cancellation: cancellation.clone(),
                })
                .or_else(|_| start())?,
            None => start()?,
        };
        let session_start = TaskSessionStartGuard::new(&self.agent_gateway, session);
        let _ownership = PreparingSessionOwnership::reserve(
            self.preparing_session_ids.clone(),
            session_start.session().key(),
        )?;
        let session_id = session_start.session().session_id.clone();
        let config_options = session_start.session().config_options.clone();
        let config_catalog = session_start.session().config_catalog.clone();
        let commands_catalog = session_start.session().commands_catalog.clone();
        let model_id = session_start.session().model_id.clone();
        let now = now_string();

        let binding = self.mutations.commit_existing_task(
            &task.task_id,
            TaskCommitOptions::metadata(),
            |ctx| {
                if ctx.task().tombstoned
                    || ctx.task().agent_session_id != task.agent_session_id
                    || !matches!(ctx.task().preparation, TaskPreparationRecord::Preparing)
                {
                    return Ok(TaskMutationResult::Rejected);
                }
                let task = ctx.task_mut();
                task.agent_session_id = Some(session_id.clone());
                if config_catalog.is_some() {
                    task.config_options = config_options.clone();
                    task.config_options_catalog = config_catalog.clone();
                    task.model_id = model_id.clone();
                }
                if task.agent_commands_catalog.is_none() {
                    task.agent_commands_catalog = commands_catalog.clone();
                }
                task.updated_at = now.clone();
                Ok(TaskMutationResult::Changed)
            },
        )?;
        if !matches!(binding.outcome, TaskCommitOutcome::Committed(_)) {
            return Err(RuntimeError::NotReady(
                "Task changed before Agent preparation completed".to_string(),
            ));
        }

        if let Err(error) =
            self.ensure_update_subscription(&task.task_id, &session_start.session().key())
        {
            self.forget_update_subscription(&task.task_id, session_start.session());
            return Err(error);
        }

        let ready_at = now_string();
        let completion = self.mutations.commit_existing_task(
            &task.task_id,
            TaskCommitOptions::metadata(),
            |ctx| {
                if ctx.task().tombstoned
                    || ctx.task().agent_session_id.as_deref() != Some(session_id.as_str())
                    || !matches!(ctx.task().preparation, TaskPreparationRecord::Preparing)
                {
                    return Ok(TaskMutationResult::Rejected);
                }
                let task = ctx.task_mut();
                task.preparation = TaskPreparationRecord::Ready;
                task.updated_at = ready_at;
                Ok(TaskMutationResult::Changed)
            },
        )?;
        if !matches!(completion.outcome, TaskCommitOutcome::Committed(_)) {
            self.forget_update_subscription(&task.task_id, session_start.session());
            return Err(RuntimeError::NotReady(
                "Task changed before Agent preparation completed".to_string(),
            ));
        }
        session_start.commit();
        Ok(())
    }

    /// Acquires the Task's Native Session and starts the accepted primary prompt in background.
    pub(crate) fn start_primary_prompt(
        &self,
        request: PrimaryPromptRequest,
    ) -> Result<(), RuntimeError> {
        let PrimaryPromptRequest {
            task,
            turn_id,
            text,
            attachments,
        } = request;
        let task_id = task.task_id.clone();
        let opened = self.acquire_for_prompt(&task)?;
        let session_id = opened.session().session_id.clone();
        let session_state = opened.task_state();
        let binding = self.mutations.commit_existing_task(
            &task_id,
            TaskCommitOptions::metadata(),
            |ctx| {
                if ctx.task().active_turn_id.as_deref() != Some(turn_id.as_str()) {
                    return Ok(TaskMutationResult::Rejected);
                }
                session_state.apply_to(ctx.task_mut());
                Ok(TaskMutationResult::Changed)
            },
        )?;
        if !matches!(binding.outcome, TaskCommitOutcome::Committed(_)) {
            return Err(RuntimeError::NotReady(
                "Native Session changed before prompt start".to_string(),
            ));
        }

        let session_sink = match self.ensure_update_subscription(&task_id, &opened.session().key())
        {
            Ok(sink) => sink,
            Err(error) => {
                self.mutations.commit_existing_task(
                    &task_id,
                    TaskCommitOptions::metadata(),
                    |ctx| {
                        if ctx.task().agent_session_id.as_deref() != Some(session_id.as_str()) {
                            return Ok(TaskMutationResult::Unchanged);
                        }
                        ctx.task_mut().agent_session_id = None;
                        Ok(TaskMutationResult::Changed)
                    },
                )?;
                return Err(error);
            }
        };
        let session = opened.commit();
        self.turn_runner.spawn_agent_turn(
            task_id,
            text,
            attachments,
            turn_id.as_str().to_string(),
            session,
            session_sink,
        );
        Ok(())
    }

    /// Loads and replaces Chat only after the caller's cached clock comparison proves staleness.
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
        match self.agent_gateway.resume_session(AgentSessionResume {
            agent_id: task.agent_id.clone(),
            task_id: task.task_id.clone(),
            session_id: stored_session_id.clone(),
            cwd: task.workspace_root.clone(),
            model_id: task.model_id.clone(),
            cancellation: TurnCancellation::new(),
        }) {
            Ok(_) => return Ok(None),
            Err(error) if is_runtime_restart_resume_gap(&error) => {}
            Err(error) => return Err(error),
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

    pub(crate) fn secret_resolver(
        &self,
        task_id: &str,
    ) -> Arc<dyn crate::agent::AgentSecretResolver> {
        task_secret_resolver(&self.server_requests, task_id)
    }

    fn acquire_for_prompt(
        &self,
        task: &TaskRecord,
    ) -> Result<OpenedNativeSession<'_>, RuntimeError> {
        self.agent_registry.require(&task.agent_id)?;
        let cancellation = TurnCancellation::new();
        match &task.agent_session_id {
            Some(session_id) => match self.agent_gateway.resume_session(AgentSessionResume {
                agent_id: task.agent_id.clone(),
                task_id: task.task_id.clone(),
                session_id: session_id.clone(),
                cwd: task.workspace_root.clone(),
                model_id: task.model_id.clone(),
                cancellation: cancellation.clone(),
            }) {
                Ok(session) => Ok(OpenedNativeSession::Resumed(session)),
                Err(_) if matches!(task.lifecycle, TaskLifecycle::New { .. }) => {
                    self.start_fresh(task, cancellation)
                }
                Err(error) if is_runtime_restart_resume_gap(&error) => self
                    .agent_gateway
                    .load_session(AgentSessionLoad {
                        agent_id: task.agent_id.clone(),
                        task_id: task.task_id.clone(),
                        session_id: session_id.clone(),
                        cwd: task.workspace_root.clone(),
                        model_id: task.model_id.clone(),
                        cancellation: cancellation.clone(),
                        secret_resolver: Some(self.secret_resolver(&task.task_id)),
                    })
                    .map(|loaded| {
                        OpenedNativeSession::Loaded(TaskSessionStartGuard::new(
                            &self.agent_gateway,
                            loaded.session,
                        ))
                    })
                    .or_else(|error| {
                        if is_restart_load_start_gap(&error) {
                            self.start_fresh(task, cancellation)
                        } else {
                            Err(error)
                        }
                    }),
                Err(error) => Err(error),
            },
            None => self.start_fresh(task, cancellation),
        }
    }

    fn start_fresh(
        &self,
        task: &TaskRecord,
        cancellation: TurnCancellation,
    ) -> Result<OpenedNativeSession<'_>, RuntimeError> {
        self.agent_gateway
            .start_session(AgentSessionStart {
                agent_id: task.agent_id.clone(),
                task_id: task.task_id.clone(),
                cwd: task.workspace_root.clone(),
                model_id: task.model_id.clone(),
                config_options: config_options_payload(task),
                config_option_policy: ConfigOptionPolicy::Strict,
                context: Vec::new(),
                cancellation,
                secret_resolver: Some(self.secret_resolver(&task.task_id)),
            })
            .map(|session| {
                OpenedNativeSession::Started(TaskSessionStartGuard::new(
                    &self.agent_gateway,
                    session,
                ))
            })
    }

    fn ensure_update_subscription(
        &self,
        task_id: &str,
        session: &AgentSessionKey,
    ) -> Result<Arc<TaskSessionEventSink>, RuntimeError> {
        let mut subscriptions = self.subscriptions.lock().map_err(|_| {
            RuntimeError::Internal("Native Session subscription lock poisoned".into())
        })?;
        if let Some(subscription) = subscriptions.get(task_id) {
            if &subscription.session == session {
                return Ok(subscription.sink.clone());
            }
        }
        let sink = self
            .turn_runner
            .attach_session_events(task_id.to_string(), session)?;
        subscriptions.insert(
            task_id.to_string(),
            NativeSessionSubscription {
                session: session.clone(),
                sink: sink.clone(),
            },
        );
        Ok(sink)
    }

    fn forget_update_subscription(&self, task_id: &str, session: &AgentSession) {
        if let Ok(mut subscriptions) = self.subscriptions.lock() {
            if subscriptions
                .get(task_id)
                .is_some_and(|subscription| subscription.session == session.key())
            {
                subscriptions.remove(task_id);
            }
        }
    }
}

struct NativeSessionSubscription {
    session: AgentSessionKey,
    sink: Arc<TaskSessionEventSink>,
}

enum OpenedNativeSession<'a> {
    Started(TaskSessionStartGuard<'a>),
    Loaded(TaskSessionStartGuard<'a>),
    Resumed(AgentSession),
}

impl OpenedNativeSession<'_> {
    fn session(&self) -> &AgentSession {
        match self {
            Self::Started(guard) | Self::Loaded(guard) => guard.session(),
            Self::Resumed(session) => session,
        }
    }

    fn task_state(&self) -> OpenedSessionTaskState {
        OpenedSessionTaskState {
            session: self.session().clone(),
            metadata_is_authoritative: matches!(self, Self::Loaded(_)),
        }
    }

    fn commit(self) -> AgentSession {
        match self {
            Self::Started(guard) | Self::Loaded(guard) => guard.commit(),
            Self::Resumed(session) => session,
        }
    }
}

struct OpenedSessionTaskState {
    session: AgentSession,
    metadata_is_authoritative: bool,
}

impl OpenedSessionTaskState {
    fn apply_to(self, task: &mut TaskRecord) {
        let AgentSession {
            session_id,
            config_options,
            config_catalog,
            commands_catalog,
            model_id,
            ..
        } = self.session;
        task.agent_session_id = Some(session_id);
        if self.metadata_is_authoritative {
            task.config_options = config_options;
            task.config_options_catalog = config_catalog;
            task.agent_commands_catalog = commands_catalog;
            task.model_id = model_id;
            return;
        }
        if let Some(catalog) = config_catalog {
            task.config_options = config_options;
            task.config_options_catalog = Some(catalog);
            task.model_id = model_id;
        } else if let Some(model_id) = model_id {
            task.model_id = Some(model_id);
        }
        if let Some(commands_catalog) = commands_catalog {
            task.agent_commands_catalog = Some(commands_catalog);
        }
    }
}

struct PreparingSessionOwnership {
    sessions: Arc<Mutex<HashSet<AgentSessionKey>>>,
    session: AgentSessionKey,
}

impl PreparingSessionOwnership {
    fn reserve(
        sessions: Arc<Mutex<HashSet<AgentSessionKey>>>,
        session: AgentSessionKey,
    ) -> Result<Self, RuntimeError> {
        sessions
            .lock()
            .map_err(|_| {
                RuntimeError::Internal("preparing session ownership lock poisoned".into())
            })?
            .insert(session.clone());
        Ok(Self { sessions, session })
    }
}

impl Drop for PreparingSessionOwnership {
    fn drop(&mut self) {
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.remove(&self.session);
        }
    }
}

fn is_runtime_restart_resume_gap(error: &RuntimeError) -> bool {
    matches!(
        error,
        RuntimeError::CapabilityMissing(capability)
            if capability == "acp_session_resume_after_runtime_restart"
    )
}

fn is_restart_load_start_gap(error: &RuntimeError) -> bool {
    matches!(error, RuntimeError::NotReady(message) if message == "ACP session start timed out")
}

fn config_options_payload(task: &TaskRecord) -> Option<serde_json::Value> {
    serde_json::to_value(&task.config_options)
        .ok()
        .filter(|value| !value.as_object().is_some_and(serde_json::Map::is_empty))
}
