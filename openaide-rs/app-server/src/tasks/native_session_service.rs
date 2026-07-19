use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use openaide_app_server_protocol::ids::TurnId;

use crate::agent::registry_handle::AgentRegistryHandle;
use crate::agent::{
    AgentPrompt, AgentSession, AgentSessionKey, AgentSessionLoad, AgentSessionResume,
    AgentSessionStart, ConfigOptionPolicy, TurnCancellation,
};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{Attachment, TaskStatus};
use crate::server_requests::ServerRequestRuntime;
use crate::storage::records::{TaskPreparationRecord, TaskRecord};
use crate::tasks::mutation::{
    TaskCommitOptions, TaskCommitOutcome, TaskMutationResult, TaskMutations,
};
use crate::tasks::product_api::secret_resolver::task_secret_resolver;
use crate::tasks::task_start_transaction::TaskSessionStartGuard;
use crate::tasks::turn_events::TaskSessionEventSink;
use crate::tasks::turns::TurnRunner;
use crate::time::now_string;

use crate::agent::gateway::AgentGateway;

mod open_recovery;
pub(crate) use open_recovery::{HistoryRefreshRequest, OpenSessionResumeOutcome};

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

    /// Reports whether this process still owns the Prepared Task's live update subscription.
    pub(crate) fn is_live(&self, task_id: &str) -> bool {
        self.subscriptions
            .lock()
            .expect("native session subscriptions poisoned")
            .contains_key(task_id)
    }

    /// Acquires and binds the empty New Task's Native Session before Composer becomes sendable.
    pub(crate) fn prepare_task(&self, task: &TaskRecord) -> Result<(), RuntimeError> {
        let cancellation = TurnCancellation::new();
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
                    secret_resolver: Some(self.secret_resolver(&task.task_id)),
                })
                .or_else(|error| {
                    if !is_session_resume_unsupported(&error) {
                        return Err(error);
                    }
                    self.agent_gateway
                        .load_session(AgentSessionLoad {
                            agent_id: task.agent_id.clone(),
                            task_id: task.task_id.clone(),
                            session_id: session_id.clone(),
                            cwd: task.workspace_root.clone(),
                            model_id: task.model_id.clone(),
                            cancellation: cancellation.clone(),
                            secret_resolver: Some(self.secret_resolver(&task.task_id)),
                        })
                        .map(|loaded| loaded.session)
                })?,
            None => self.agent_gateway.start_session(AgentSessionStart {
                agent_id: task.agent_id.clone(),
                task_id: task.task_id.clone(),
                cwd: task.workspace_root.clone(),
                model_id: task.model_id.clone(),
                config_options: config_options_payload(task),
                config_option_policy: ConfigOptionPolicy::ReconcileWithAgentDefaults,
                context: Vec::new(),
                cancellation: cancellation.clone(),
                secret_resolver: Some(self.secret_resolver(&task.task_id)),
            })?,
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
        let supports_image_input = session_start.session().prompt_capabilities.image;
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
                task.supports_image_input = supports_image_input;
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

        let session_sink = self.ensure_update_subscription(&task_id, &opened.session().key())?;
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

    /// Restores an existing Task binding before session-scoped interactions such as
    /// Configuration Option changes. Recovery stays behind this module so product
    /// workflows do not need to choose between ACP resume and load.
    pub(crate) fn ensure_active_for_interaction(
        &self,
        task: &TaskRecord,
    ) -> Result<AgentSessionKey, RuntimeError> {
        let expected_session_id = task.agent_session_id.as_deref().ok_or_else(|| {
            RuntimeError::NotReady("Task has no Native Session to recover".to_string())
        })?;
        let opened = self.acquire_for_prompt(task)?;
        let session_key = opened.session().key();
        let session_state = opened.task_state();
        let binding = self.mutations.commit_existing_task(
            &task.task_id,
            TaskCommitOptions::metadata(),
            |ctx| {
                if ctx.task().tombstoned
                    || ctx.task().agent_session_id.as_deref() != Some(expected_session_id)
                {
                    return Ok(TaskMutationResult::Rejected);
                }
                session_state.apply_to(ctx.task_mut());
                Ok(TaskMutationResult::Changed)
            },
        )?;
        if !matches!(binding.outcome, TaskCommitOutcome::Committed(_)) {
            return Err(RuntimeError::NotReady(
                "Native Session changed during recovery".to_string(),
            ));
        }
        self.ensure_update_subscription(&task.task_id, &session_key)?;
        opened.commit();
        Ok(session_key)
    }

    /// Sends steering to the already-working Native Session without creating
    /// another App Server-owned work lifecycle or awaiting its response.
    pub(crate) fn steer(
        &self,
        task: TaskRecord,
        text: String,
        attachments: Vec<Attachment>,
    ) -> Result<(), RuntimeError> {
        self.agent_registry.require(&task.agent_id)?;
        let session_id = task.agent_session_id.ok_or_else(|| {
            RuntimeError::NotReady("Working Task has no active Native Session".to_string())
        })?;
        self.turn_runner.steer_session(AgentPrompt {
            agent_id: task.agent_id,
            task_id: task.task_id,
            session_id,
            text,
            attachments,
            cancellation: TurnCancellation::new(),
        })
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
                secret_resolver: Some(self.secret_resolver(&task.task_id)),
            }) {
                Ok(session) => Ok(OpenedNativeSession::Resumed(session)),
                Err(error) if is_session_resume_unsupported(&error) => self
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
                self.turn_runner
                    .reattach_session_events(session, &subscription.sink)?;
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
            prompt_capabilities,
            prompt_capabilities_authoritative,
            ..
        } = self.session;
        task.agent_session_id = Some(session_id);
        if prompt_capabilities_authoritative {
            task.supports_image_input = prompt_capabilities.image;
        }
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

fn is_session_resume_unsupported(error: &RuntimeError) -> bool {
    matches!(error, RuntimeError::CapabilityMissing(_))
}

fn config_options_payload(task: &TaskRecord) -> Option<serde_json::Value> {
    serde_json::to_value(&task.config_options)
        .ok()
        .filter(|value| !value.as_object().is_some_and(serde_json::Map::is_empty))
}
