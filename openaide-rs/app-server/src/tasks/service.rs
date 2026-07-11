use std::sync::{Arc, Mutex};

use crate::agent::gateway::AgentGateway;
use crate::agent::registry::AgentRegistry;
use crate::agent::status_cache::AgentStatusCache;
use crate::agent::AgentRuntime;
use crate::diagnostics::TaskDiagnostics;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{
    ActivityToolDetails, AgentAuthenticateResult, AgentListSessionsResult, AgentProbeResult,
    MessagePage, TaskSnapshot,
};
use crate::protocol::params::{
    AgentAuthenticateParams, AgentListSessionsParams, AgentProbeParams, ChatPageParams,
    ChatTailParams, PermissionRespondParams, SessionPromptParams, TaskCreateParams,
    TaskDeleteParams, TaskIdParams, TaskListParams, TaskSnapshotParams, ToolDetailParams,
};
use crate::protocol::results::TaskListResult;
use crate::storage::Store;
use crate::task_events::TaskUpdateNotifier;
use crate::tasks::agent_service::AgentService;
use crate::tasks::mutation::TaskMutations;
use crate::tasks::query::TaskQueries;
use crate::tasks::query_store::TaskReadStore;
use crate::tasks::revision_source::TaskRevisionSource;
use crate::tasks::runtime_state::RuntimeState;
use crate::tasks::task_commands::TaskCommands;
use crate::tasks::turn_lifecycle::TaskTurnLifecycle;
use crate::tasks::turns::TurnRunner;
use serde_json::Value;

pub struct TaskService {
    store: Store,
    store_update_lock: Arc<Mutex<()>>,
    mutations: TaskMutations,
    agent_gateway: AgentGateway,
    turn_runner: TurnRunner,
    agent_registry: AgentRegistry,
    agent_service: AgentService,
    queries: TaskQueries,
    commands: TaskCommands,
}

impl TaskService {
    pub fn open(store: Store, agent: Arc<dyn AgentRuntime>) -> Result<Self, RuntimeError> {
        Self::open_with_task_update_notifier(store, agent, TaskUpdateNotifier::disabled())
    }

    pub fn open_with_task_update_notifier(
        store: Store,
        agent: Arc<dyn AgentRuntime>,
        task_update_notifier: TaskUpdateNotifier,
    ) -> Result<Self, RuntimeError> {
        Self::open_with_task_update_notifier_and_agent_registry(
            store,
            agent,
            task_update_notifier,
            AgentRegistry::default_built_ins(),
        )
    }

    pub(crate) fn open_with_task_update_notifier_and_agent_registry(
        store: Store,
        agent: Arc<dyn AgentRuntime>,
        task_update_notifier: TaskUpdateNotifier,
        agent_registry: AgentRegistry,
    ) -> Result<Self, RuntimeError> {
        Self::open_with_task_update_notifier_and_agent_registry_and_status_cache(
            store,
            agent,
            task_update_notifier,
            agent_registry,
            AgentStatusCache::default(),
        )
    }

    pub(crate) fn open_with_task_update_notifier_and_agent_registry_and_status_cache(
        store: Store,
        agent: Arc<dyn AgentRuntime>,
        task_update_notifier: TaskUpdateNotifier,
        agent_registry: AgentRegistry,
        agent_statuses: AgentStatusCache,
    ) -> Result<Self, RuntimeError> {
        let initial_revision = store.max_task_revision()?;
        let service = Self::with_initial_revision(
            store,
            agent,
            initial_revision,
            task_update_notifier,
            agent_registry,
            agent_statuses,
        );
        service.recover_volatile_runtime_state()?;
        Ok(service)
    }

    pub fn new(store: Store, agent: Arc<dyn AgentRuntime>) -> Self {
        Self::with_initial_revision(
            store,
            agent,
            0,
            TaskUpdateNotifier::disabled(),
            AgentRegistry::default_built_ins(),
            AgentStatusCache::default(),
        )
    }

    fn with_initial_revision(
        store: Store,
        agent: Arc<dyn AgentRuntime>,
        initial_revision: u64,
        notifier: TaskUpdateNotifier,
        agent_registry: AgentRegistry,
        agent_statuses: AgentStatusCache,
    ) -> Self {
        let runtime_state = Arc::new(Mutex::new(RuntimeState::with_revision(initial_revision)));
        let revision_source = TaskRevisionSource::new(runtime_state.clone());
        let store_update_lock = Arc::new(Mutex::new(()));
        let mutations = TaskMutations::new(
            store.clone(),
            store_update_lock.clone(),
            runtime_state.clone(),
            notifier.clone(),
        );
        let agent_gateway = AgentGateway::new(agent.clone());
        let turn_runner = TurnRunner::new(mutations.clone(), agent);
        let agent_service = AgentService::with_status_cache(
            agent_gateway.clone(),
            agent_registry.clone(),
            agent_statuses.clone(),
        );
        let queries = TaskQueries::new(
            TaskReadStore::new(store.clone()),
            store_update_lock.clone(),
            revision_source,
        );
        let commands = TaskCommands::new(mutations.clone(), agent_gateway.clone());
        Self {
            store,
            store_update_lock,
            mutations,
            agent_gateway,
            turn_runner,
            agent_registry,
            agent_service,
            queries,
            commands,
        }
    }

    pub fn list(&self, params: TaskListParams) -> Result<TaskListResult, RuntimeError> {
        self.queries.list(params)
    }

    pub fn diagnostics(&self) -> Result<TaskDiagnostics, RuntimeError> {
        self.queries.diagnostics()
    }

    pub fn create(&self, params: TaskCreateParams) -> Result<TaskSnapshot, RuntimeError> {
        self.turn_lifecycle().create(params)
    }

    pub fn snapshot(&self, params: TaskSnapshotParams) -> Result<TaskSnapshot, RuntimeError> {
        self.queries.snapshot(params)
    }

    pub fn tail(&self, params: ChatTailParams) -> Result<MessagePage, RuntimeError> {
        self.queries.tail(params)
    }

    pub fn page(&self, params: ChatPageParams) -> Result<MessagePage, RuntimeError> {
        self.queries.page(params)
    }

    pub fn tool_detail(
        &self,
        params: ToolDetailParams,
    ) -> Result<ActivityToolDetails, RuntimeError> {
        self.queries.tool_detail(params)
    }

    pub fn prompt(&self, params: SessionPromptParams) -> Result<TaskSnapshot, RuntimeError> {
        self.turn_lifecycle().prompt(params)
    }

    pub fn cancel(&self, params: TaskIdParams) -> Result<TaskSnapshot, RuntimeError> {
        self.turn_lifecycle().cancel(params)
    }

    pub fn respond_permission(
        &self,
        params: PermissionRespondParams,
    ) -> Result<TaskSnapshot, RuntimeError> {
        self.turn_lifecycle().respond_permission(params)
    }

    pub fn mark_read(&self, params: TaskIdParams) -> Result<TaskSnapshot, RuntimeError> {
        self.commands.mark_read(params)
    }

    pub fn delete(&self, params: TaskDeleteParams) -> Result<Value, RuntimeError> {
        self.commands.delete(params)
    }

    pub fn shutdown(&self) -> Result<(), RuntimeError> {
        self.turn_lifecycle().shutdown()?;
        self.store.mark_clean_shutdown()
    }

    pub fn probe_agent(&self, params: AgentProbeParams) -> Result<AgentProbeResult, RuntimeError> {
        self.agent_service.probe(params)
    }

    pub fn authenticate_agent(
        &self,
        params: AgentAuthenticateParams,
    ) -> Result<AgentAuthenticateResult, RuntimeError> {
        self.agent_service.authenticate(params)
    }

    pub fn list_agent_sessions(
        &self,
        params: AgentListSessionsParams,
    ) -> Result<AgentListSessionsResult, RuntimeError> {
        self.agent_service.list_sessions(params)
    }

    fn turn_lifecycle(&self) -> TaskTurnLifecycle {
        TaskTurnLifecycle::new(
            self.store.clone(),
            self.store_update_lock.clone(),
            self.mutations.clone(),
            self.agent_gateway.clone(),
            self.turn_runner.clone(),
            self.agent_registry.clone(),
        )
    }

    fn recover_volatile_runtime_state(&self) -> Result<(), RuntimeError> {
        self.turn_lifecycle().recover_volatile_runtime_state()
    }
}
