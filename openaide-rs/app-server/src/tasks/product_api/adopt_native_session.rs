use uuid::Uuid;

use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::snapshot::TaskSnapshot;
use openaide_app_server_protocol::task::TaskAdoptNativeSessionParams;

use crate::agent::{AgentLoadedSession, AgentSessionLoad, TurnCancellation};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{
    IsolationKind, TaskSnapshot as StoredTaskSnapshot, TaskStatus as LegacyTaskStatus,
};
use crate::storage::records::{
    TaskLifecycle, TaskPreparationRecord, TaskRecord, TaskTitle, TaskTitleSource,
};
use crate::tasks::mutation::TaskCommitOptions;
use crate::tasks::mutation::TaskMutationResult;
use crate::tasks::task_start_transaction::TaskSessionStartGuard;
use crate::tasks::transitions::TaskTransitions;
use crate::time::now_string;

use super::{protocol_error_from_runtime, TaskProductApi};

/// What the admission step decided, captured while the Native Session gate was held.
enum Admission {
    /// A live Task already owns this Native Session; adoption is idempotent.
    Existing(Box<TaskSnapshot>),
    New(Box<AdoptionPlan>),
}

/// Everything the slow load and the commit need, so neither re-reads the catalog.
struct AdoptionPlan {
    catalog_entry: crate::native_sessions::catalog::NativeSessionCatalogEntry,
    reference: crate::native_sessions::catalog::NativeSessionRef,
    project_root: String,
    isolation: IsolationKind,
    now: String,
    last_activity: String,
    task_id: String,
    title: Option<TaskTitle>,
}

/// Start and terminal events for one adoption stage. Metadata only: ids, outcome, duration.
struct AdoptionStage<'a> {
    stage: &'static str,
    params: &'a TaskAdoptNativeSessionParams,
    task_id: &'a str,
    started_at: std::time::Instant,
}

impl<'a> AdoptionStage<'a> {
    fn begin(
        stage: &'static str,
        params: &'a TaskAdoptNativeSessionParams,
        task_id: &'a str,
    ) -> Self {
        crate::logging::info(
            "native_session_adoption_stage_started",
            serde_json::json!({
                "stage": stage,
                "task_id": task_id,
                "agent_id": params.agent_id.as_str(),
                "session_id": params.native_session_id.as_str(),
            }),
        );
        Self {
            stage,
            params,
            task_id,
            started_at: std::time::Instant::now(),
        }
    }

    fn finish<T, E>(self, result: &Result<T, E>, counts: serde_json::Value) {
        let fields = serde_json::json!({
            "stage": self.stage,
            "task_id": self.task_id,
            "agent_id": self.params.agent_id.as_str(),
            "session_id": self.params.native_session_id.as_str(),
            "outcome": if result.is_ok() { "completed" } else { "failed" },
            "duration_ms": self.started_at.elapsed().as_millis(),
            "counts": counts,
        });
        if result.is_ok() {
            crate::logging::info("native_session_adoption_stage_finished", fields);
        } else {
            crate::logging::warn("native_session_adoption_stage_finished", fields);
        }
    }
}

impl TaskProductApi {
    /// Adopts one Native Session as a Task, or returns the Task that already owns it.
    ///
    /// Callers must not hold the protocol lock: the Agent `session/load`, the replay
    /// projection and the Subagent restore take seconds. Concurrency:
    /// - Two requests for the same (agent, session) serialize on a keyed gate held for the
    ///   whole adoption. The second waits for the first, then finds its Task in admission and
    ///   returns it, so one session never becomes two Tasks. (The persist step also
    ///   re-checks ownership as a backstop.)
    /// - Different sessions adopt in parallel; the shared Native Session mutation gate
    ///   (`native_adoption`, also taken by archive, delete, fork and metadata edits) is held
    ///   only for admission and for the commit, never across the Agent round-trip, so those
    ///   requests are not stalled behind a slow load.
    /// - Anything that changed while the Agent loaded (the catalog row removed, a deletion
    ///   started) is re-checked at commit; the loaded session is then closed and the request
    ///   fails without creating a Task.
    pub(super) fn adopt_native_session_as_task(
        &self,
        params: TaskAdoptNativeSessionParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        let started_at = std::time::Instant::now();
        crate::logging::info(
            "native_session_adoption_started",
            serde_json::json!({
                "agent_id": params.agent_id.as_str(),
                "session_id": params.native_session_id.as_str(),
            }),
        );
        let gate = format!(
            "adopt:{}:{}",
            params.agent_id.as_str(),
            params.native_session_id
        );
        let result = self
            .session_operations
            .serialize(&gate, || self.adopt_serialized(&params));
        let fields = serde_json::json!({
            "agent_id": params.agent_id.as_str(),
            "session_id": params.native_session_id.as_str(),
            "outcome": if result.is_ok() { "completed" } else { "failed" },
            "error_code": result.as_ref().err().map(|error| format!("{:?}", error.code)),
            "duration_ms": started_at.elapsed().as_millis(),
        });
        if result.is_ok() {
            crate::logging::info("native_session_adoption_finished", fields);
        } else {
            crate::logging::warn("native_session_adoption_finished", fields);
        }
        result
    }

    fn lock_native_adoption(&self) -> Result<std::sync::MutexGuard<'_, ()>, ProtocolError> {
        self.native_adoption.lock().map_err(|_| {
            protocol_error_from_runtime(RuntimeError::Internal(
                "Native Session adoption lock poisoned".to_string(),
            ))
        })
    }

    fn adopt_serialized(
        &self,
        params: &TaskAdoptNativeSessionParams,
    ) -> Result<TaskSnapshot, ProtocolError> {
        let admission = {
            let _gate = self.lock_native_adoption()?;
            self.admit_adoption(params)?
        };
        let plan = match admission {
            Admission::Existing(snapshot) => return Ok(*snapshot),
            Admission::New(plan) => plan,
        };
        let secret_resolver = self.task_secret_resolver_for_project(
            &plan.task_id,
            openaide_app_server_protocol::ids::ProjectId::from(
                plan.catalog_entry.project_id.clone(),
            ),
        );
        let stage = AdoptionStage::begin("agent_load", params, &plan.task_id);
        let load_result = self.agent_gateway.load_session(AgentSessionLoad {
            agent_id: params.agent_id.as_str().to_string(),
            task_id: plan.task_id.clone(),
            cwd: plan.catalog_entry.workspace_root.clone(),
            model_id: None,
            session_id: params.native_session_id.clone(),
            cancellation: TurnCancellation::new(),
            secret_resolver: Some(secret_resolver),
        });
        stage.finish(
            &load_result,
            match &load_result {
                Ok(loaded) => serde_json::json!({
                    "replayed_message_count": loaded.replayed_messages.len(),
                    "replayed_subagent_event_count": loaded.replayed_subagents.len(),
                }),
                Err(_) => serde_json::Value::Null,
            },
        );
        let mut loaded = match load_result {
            Ok(loaded) => loaded,
            Err(error @ RuntimeError::NativeSessionMissing(_)) => {
                let _gate = self.lock_native_adoption()?;
                if self.native_catalog.remove(&plan.reference).unwrap_or(false) {
                    self.task_notifier
                        .navigation_project_changed(plan.catalog_entry.project_id.clone());
                }
                return Err(protocol_error_from_runtime(error));
            }
            Err(error) => return Err(protocol_error_from_runtime(error)),
        };
        let mut session_start =
            TaskSessionStartGuard::new(&self.agent_gateway, loaded.session.clone());
        let session_id = session_start.session_id().to_string();
        let replayed_subagents = std::mem::take(&mut loaded.replayed_subagents);

        let replayed_message_count = loaded.replayed_messages.len();
        let stage = AdoptionStage::begin("commit", params, &plan.task_id);
        let mut gate_wait_ms = 0_u64;
        let persist_result = (|| {
            let gate_wait = std::time::Instant::now();
            let _gate = self.lock_native_adoption().map_err(|error| {
                RuntimeError::Internal(format!("adoption gate unavailable: {:?}", error.code))
            })?;
            gate_wait_ms = gate_wait.elapsed().as_millis() as u64;
            // The load ran without the gate: re-check what it could have changed.
            self.require_session_not_deleting(&plan.reference)
                .map_err(|error| RuntimeError::Conflict(error.message))?;
            if self.native_catalog.entry(&plan.reference).is_none() {
                return Err(RuntimeError::TaskNotFound(
                    "Native Session is no longer available".to_string(),
                ));
            }
            self.persist_adopted_session_task(
                params,
                &plan.catalog_entry.workspace_root,
                &plan.project_root,
                plan.isolation,
                &plan.task_id,
                &plan.now,
                &plan.last_activity,
                plan.title.clone(),
                plan.catalog_entry.pinned,
                &session_id,
                loaded,
            )
        })();
        // The Task's own `task_create_*` events (same Task id) break the commit down further.
        stage.finish(
            &persist_result,
            serde_json::json!({
                "message_count": replayed_message_count,
                "gate_wait_ms": gate_wait_ms,
            }),
        );
        let snapshot = match persist_result {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let _ = session_start.close();
                let _ = self.fail_adopted_task_attach(&plan.task_id, &session_id, &error);
                return Err(protocol_error_from_runtime(error));
            }
        };

        // Attaching only subscribes the Task's sink to the session. The replayed Subagents
        // are restored in the background after the reply's snapshot exists: the Chat does
        // not need them, and they reach clients as ordinary Subagent notifications.
        let stage = AdoptionStage::begin("attach", params, &plan.task_id);
        let attached = self
            .turn_runner
            .attach_session_events(plan.task_id.clone(), &session_start.session().key());
        stage.finish(&attached, serde_json::Value::Null);
        let sink = match attached {
            Ok(sink) => sink,
            Err(error) => {
                let _ = session_start.close();
                if let Err(finalize_error) =
                    self.fail_adopted_task_attach(&plan.task_id, &session_id, &error)
                {
                    return Err(protocol_error_from_runtime(RuntimeError::Internal(format!(
                        "{error}; failed to finalize adopted task after session event attachment failure: {finalize_error}"
                    ))));
                }
                return Err(protocol_error_from_runtime(error));
            }
        };
        let _session = session_start.commit();
        let stage = AdoptionStage::begin("snapshot", params, &plan.task_id);
        let projected = self.project_task_snapshot(snapshot);
        stage.finish(&projected, serde_json::Value::Null);
        // The Task and its session are committed whether or not the reply's snapshot built.
        self.turn_runner.restore_replayed_subagents_in_background(
            &plan.task_id,
            sink,
            replayed_subagents,
        );
        projected
    }

    /// Validates the request and captures what the load and commit need. Runs under the
    /// Native Session gate.
    fn admit_adoption(
        &self,
        params: &TaskAdoptNativeSessionParams,
    ) -> Result<Admission, ProtocolError> {
        let reference = crate::native_sessions::catalog::NativeSessionRef::new(
            params.agent_id.as_str(),
            &params.native_session_id,
        );
        self.require_session_not_deleting(&reference)?;
        if let Some(existing) = self
            .store
            .list_all_task_records_strict()
            .map_err(protocol_error_from_runtime)?
            .into_iter()
            .find(|task| {
                !task.tombstoned
                    && task.agent_id == params.agent_id.as_str()
                    && task.agent_session_id.as_deref() == Some(params.native_session_id.as_str())
            })
        {
            let result = self
                .mutations
                .commit_existing_task(
                    &existing.task_id,
                    super::response_snapshot_options(),
                    |_| Ok(TaskMutationResult::Unchanged),
                )
                .map_err(protocol_error_from_runtime)?;
            return self
                .project_task_snapshot(result.response_snapshot.ok_or_else(|| {
                    super::internal_error("missing existing adopted Task snapshot")
                })?)
                .map(|snapshot| Admission::Existing(Box::new(snapshot)));
        }
        let catalog_entry = self.native_catalog.entry(&reference).ok_or_else(|| {
            crate::logging::info(
                "native_session_adoption_catalog_miss",
                serde_json::json!({
                    "agent_id": params.agent_id.as_str(),
                    "session_id": params.native_session_id.as_str(),
                }),
            );
            protocol_error_from_runtime(RuntimeError::TaskNotFound(
                "Native Session is no longer available".to_string(),
            ))
        })?;
        let project = self.project_resolver.resolve_task_context(
            &openaide_app_server_protocol::ids::ProjectId::from(catalog_entry.project_id.clone()),
        )?;
        self.agent_registry
            .require(params.agent_id.as_str())
            .map_err(protocol_error_from_runtime)?;
        self.mutations
            .ensure_native_session_unowned(params.agent_id.as_str(), &params.native_session_id)
            .map_err(protocol_error_from_runtime)?;

        let now = now_string();
        // Preserve the activity time shown in Native Session navigation when the row becomes a
        // Task. Adoption time is persistence metadata, not new conversation activity.
        let last_activity = catalog_entry
            .observation
            .last_activity
            .clone()
            .unwrap_or_else(|| now.clone());
        let title = catalog_entry
            .user_title
            .clone()
            .and_then(|title| TaskTitle::new(title, TaskTitleSource::User))
            .or_else(|| {
                catalog_entry
                    .observation
                    .title
                    .clone()
                    .or(catalog_entry.local_fallback_title.clone())
                    .and_then(|title| TaskTitle::new(title, TaskTitleSource::Agent))
            });
        let isolation = if catalog_entry.workspace_root == project.workspace_root {
            project.isolation
        } else {
            IsolationKind::GitWorktree
        };
        Ok(Admission::New(Box::new(AdoptionPlan {
            reference,
            project_root: project.workspace_root.clone(),
            isolation,
            now,
            last_activity,
            task_id: format!("task_{}", Uuid::new_v4()),
            title,
            catalog_entry,
        })))
    }

    #[allow(clippy::too_many_arguments)]
    fn persist_adopted_session_task(
        &self,
        params: &TaskAdoptNativeSessionParams,
        workspace_root: &str,
        project_root: &str,
        isolation: IsolationKind,
        task_id: &str,
        now: &str,
        last_activity: &str,
        title: Option<TaskTitle>,
        pinned: bool,
        session_id: &str,
        loaded: AgentLoadedSession,
    ) -> Result<StoredTaskSnapshot, RuntimeError> {
        let selected_agent_id = params.agent_id.as_str().to_string();
        let workspace_root = workspace_root.to_string();
        let session_id = session_id.to_string();
        let record = TaskRecord {
            task_id: task_id.to_string(),
            title: crate::storage::records::TaskTitleState::from_title(title),
            status: LegacyTaskStatus::Inactive,
            task_version: 1,
            message_history_version: 0,
            unread: false,
            pinned,
            attention: None,
            created_at: now.to_string(),
            updated_at: now.to_string(),
            last_activity: last_activity.to_string(),
            permission_policy: Default::default(),
            composer_history: Default::default(),
            message_queue: Default::default(),
            agent_name: self
                .agent_registry
                .display_name(params.agent_id.as_str(), None)?,
            agent_id: selected_agent_id.clone(),
            isolation,
            workspace_root: workspace_root.clone(),
            project_root: Some(project_root.to_string()),
            worktree_id: None,
            lifecycle: TaskLifecycle::Open,
            agent_session_id: Some(session_id.clone()),
            active_turn_id: None,
            active_turn_started_at: None,
            background_work: None,
            tombstoned: false,
            revision: 0,
            config_options_catalog: loaded.session.config_catalog.clone(),
            native_session_data_freshness: Default::default(),
            native_session_reload_requirement: None,
            native_session_own_activity_at: None,
            config_mutation: Default::default(),
            agent_commands_catalog: loaded.session.commands_catalog.clone(),
            context_usage: None,
            current_plan: loaded.session.replayed_plan.current_plan.clone(),
            completed_plan_message_id: loaded
                .session
                .replayed_plan
                .completed_plan_message_id
                .clone(),
            last_turn_usage: None,
            model_id: loaded.session.model_id.clone(),
            supports_image_input: loaded.session.prompt_capabilities.image,
            preparation: TaskPreparationRecord::Ready,
        };
        let result = self.mutations.create_task_with_validation(
            record,
            loaded.replayed_messages,
            TaskCommitOptions {
                refresh_message_history: false,
                response_snapshot_tail_limit: Some(100),
            },
            |validation| validation.ensure_native_session_unowned(&selected_agent_id, &session_id),
        )?;
        result
            .response_snapshot
            .ok_or_else(|| RuntimeError::Internal("missing adopted task snapshot".to_string()))
    }

    fn fail_adopted_task_attach(
        &self,
        task_id: &str,
        session_id: &str,
        error: &RuntimeError,
    ) -> Result<(), RuntimeError> {
        TaskTransitions::new(self.mutations.clone(), self.server_requests.clone())
            .fail_adopted_task_attach(task_id, session_id, error)
    }
}
