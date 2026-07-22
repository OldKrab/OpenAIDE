use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use tempfile::TempDir;

use crate::agent::{AgentEventSink, AgentPrompt, AgentRuntime, AgentSession, AgentSessionStart};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{IsolationKind, TaskStatus};
use crate::protocol::params::{TaskCreateMode, TaskCreateParams};
use crate::storage::task_journal::TaskWrite;
use crate::storage::Store;
use crate::tasks::TaskService;

#[test]
fn shutdown_continues_cleanup_after_one_task_transition_fails() {
    let temp = TempDir::new().unwrap();
    let store = Store::open(temp.path().join("store")).unwrap();
    let agent = Arc::new(ShutdownContinuationAgent::default());
    let service = TaskService::new(store.clone(), agent.clone());
    let first = service
        .create(task_params(temp.path(), "First task"))
        .unwrap();
    let second = service
        .create(task_params(temp.path(), "Second task"))
        .unwrap();
    agent.wait_for_prompt_count(2);
    store.fail_next_task_write_for_test();

    let shutdown_result = Arc::new(Mutex::new(None));
    let (shutdown_entered, statuses_before_agent_release) = thread::scope(|scope| {
        let shutdown_result = shutdown_result.clone();
        let state = agent.state.clone();
        scope.spawn(move || {
            let result = service.shutdown();
            *shutdown_result.lock().expect("shutdown result poisoned") = Some(result);
            let (state_lock, changed) = &*state;
            state_lock
                .lock()
                .expect("shutdown state poisoned")
                .shutdown_finished = true;
            changed.notify_all();
        });

        let shutdown_entered = agent.wait_for_shutdown_or_completion();
        let statuses = [first.task.task_id, second.task.task_id].map(|task_id| {
            store
                .read_task(&task_id)
                .expect("Task remains readable during shutdown")
                .status
        });
        agent.release();
        (shutdown_entered, statuses)
    });

    let result = shutdown_result
        .lock()
        .expect("shutdown result poisoned")
        .take()
        .expect("shutdown thread returned a result");
    assert!(
        matches!(
            result,
            Err(RuntimeError::Storage(message))
                if message == "injected Task record write failure"
        ),
        "the first persistence failure remains observable"
    );
    assert!(
        shutdown_entered,
        "Agent shutdown must still run after a Task transition fails"
    );
    assert_eq!(
        statuses_before_agent_release
            .iter()
            .filter(|status| **status == TaskStatus::Inactive)
            .count(),
        1,
        "the Task after the failed transition must still be finalized"
    );
}

#[cfg(unix)]
#[test]
fn task_storage_failure_cancels_the_live_agent_turn() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().unwrap();
    let state_root = temp.path().join("store");
    let store = Store::open(state_root.clone()).unwrap();
    let agent = Arc::new(StorageFailureCancellationAgent::default());
    let service = TaskService::new(store.clone(), agent.clone());
    let created = service
        .create(task_params(temp.path(), "Storage failure cancellation"))
        .unwrap();
    agent.wait_for_prompt();

    let task_id = created.task.task_id;
    let task_dir = state_root.join("task-store-v1/tasks").join(&task_id);
    std::fs::set_permissions(&task_dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let mut task = store.read_task(&task_id).unwrap();
    task.status = TaskStatus::Completed;
    store
        .task_journal()
        .submit(TaskWrite::barrier_replace_task(task))
        .unwrap()
        .wait()
        .expect_err("write failure freezes the Task");

    agent.wait_for_cancel();
    std::fs::set_permissions(&task_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(agent.cancel_count.load(Ordering::SeqCst), 1);
}

fn task_params(workspace: &std::path::Path, title: &str) -> TaskCreateParams {
    TaskCreateParams {
        mode: TaskCreateMode::PromptStart,
        title: title.to_string(),
        workspace_root: workspace.to_string_lossy().to_string(),
        selected_agent_id: "codex".to_string(),
        selected_agent_label: None,
        selected_isolation: IsolationKind::Local,
        prompt_text: Some(format!("Keep {title} active")),
        external_session_id: None,
        model_id: None,
        context: Vec::new(),
    }
}

#[derive(Default)]
struct ShutdownContinuationAgent {
    next_session: AtomicUsize,
    state: Arc<(Mutex<ShutdownContinuationState>, Condvar)>,
}

#[derive(Default)]
struct ShutdownContinuationState {
    prompt_count: usize,
    shutdown_entered: bool,
    shutdown_finished: bool,
    release: bool,
}

impl ShutdownContinuationAgent {
    fn wait_for_prompt_count(&self, expected: usize) {
        self.wait_until(|state| state.prompt_count == expected);
    }

    fn wait_for_shutdown_or_completion(&self) -> bool {
        self.wait_until(|state| state.shutdown_entered || state.shutdown_finished);
        self.state
            .0
            .lock()
            .expect("shutdown state poisoned")
            .shutdown_entered
    }

    fn release(&self) {
        let (state_lock, changed) = &*self.state;
        state_lock.lock().expect("shutdown state poisoned").release = true;
        changed.notify_all();
    }

    fn wait_until(&self, predicate: impl Fn(&ShutdownContinuationState) -> bool) {
        let (state_lock, changed) = &*self.state;
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut state = state_lock.lock().expect("shutdown state poisoned");
        while !predicate(&state) {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .expect("timed out waiting for shutdown state");
            let (next, timeout) = changed
                .wait_timeout(state, remaining)
                .expect("shutdown state poisoned");
            state = next;
            assert!(!timeout.timed_out(), "timed out waiting for shutdown state");
        }
    }
}

impl AgentRuntime for ShutdownContinuationAgent {
    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        let session = self.next_session.fetch_add(1, Ordering::SeqCst);
        Ok(AgentSession::new(
            request.agent_id,
            format!("shutdown-session-{session}"),
        ))
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<crate::agent::AgentPromptOutcome, RuntimeError> {
        let (state_lock, changed) = &*self.state;
        let mut state = state_lock.lock().expect("shutdown state poisoned");
        state.prompt_count += 1;
        changed.notify_all();
        while !state.release {
            state = changed.wait(state).expect("shutdown state poisoned");
        }
        Ok(crate::agent::AgentPromptOutcome::EndTurn)
    }

    fn shutdown(&self) -> Result<(), RuntimeError> {
        let (state_lock, changed) = &*self.state;
        let mut state = state_lock.lock().expect("shutdown state poisoned");
        state.shutdown_entered = true;
        changed.notify_all();
        while !state.release {
            state = changed.wait(state).expect("shutdown state poisoned");
        }
        Ok(())
    }
}

#[derive(Default)]
struct StorageFailureCancellationAgent {
    state: Arc<(Mutex<StorageFailureCancellationState>, Condvar)>,
    cancel_count: AtomicUsize,
}

#[derive(Default)]
struct StorageFailureCancellationState {
    prompt_started: bool,
    cancelled: bool,
}

impl StorageFailureCancellationAgent {
    fn wait_for_prompt(&self) {
        self.wait_until(|state| state.prompt_started);
    }

    fn wait_for_cancel(&self) {
        self.wait_until(|state| state.cancelled);
    }

    fn wait_until(&self, condition: impl Fn(&StorageFailureCancellationState) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(2);
        let (state, changed) = &*self.state;
        let mut state = state.lock().unwrap();
        while !condition(&state) {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .expect("timed out waiting for storage-failure cancellation");
            let (next, timeout) = changed.wait_timeout(state, remaining).unwrap();
            state = next;
            assert!(!timeout.timed_out());
        }
    }
}

impl AgentRuntime for StorageFailureCancellationAgent {
    fn start_session(&self, request: AgentSessionStart) -> Result<AgentSession, RuntimeError> {
        Ok(AgentSession::new(
            request.agent_id,
            "storage-failure-session",
        ))
    }

    fn prompt(
        &self,
        _prompt: AgentPrompt,
        _sink: Arc<dyn AgentEventSink>,
    ) -> Result<crate::agent::AgentPromptOutcome, RuntimeError> {
        let (state, changed) = &*self.state;
        let mut state = state.lock().unwrap();
        state.prompt_started = true;
        changed.notify_all();
        while !state.cancelled {
            state = changed.wait(state).unwrap();
        }
        Ok(crate::agent::AgentPromptOutcome::Cancelled)
    }

    fn cancel_session(&self, _session: &crate::agent::AgentSessionKey) -> Result<(), RuntimeError> {
        self.cancel_count.fetch_add(1, Ordering::SeqCst);
        let (state, changed) = &*self.state;
        state.lock().unwrap().cancelled = true;
        changed.notify_all();
        Ok(())
    }
}
