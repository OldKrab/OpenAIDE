use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::{AgentId, ClientInstanceId, ProjectId, TaskId};
use openaide_app_server_protocol::snapshot::NativeSessionReference;
use openaide_app_server_protocol::task::{
    NativeSessionDeleteParams, NativeSessionDeleteResult, NativeSessionDeleteTarget,
};

use crate::agent::{AgentProbeRequest, AgentSessionDelete};
use crate::native_sessions::catalog::NativeSessionRef;
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::TaskStatus;
use crate::storage::records::{TaskLifecycle, TaskRecord};
use crate::tasks::mutation::{TaskCommitOptions, TaskCommitOutcome, TaskMutationResult};

use super::{conflict_error, protocol_error_from_runtime, TaskProductApi};

struct DeletionTarget {
    reference: NativeSessionRef,
    project_id: ProjectId,
    title: String,
    task: Option<TaskRecord>,
}

impl TaskProductApi {
    pub(super) fn require_session_not_deleting(
        &self,
        reference: &NativeSessionRef,
    ) -> Result<(), ProtocolError> {
        if self.turn_acceptance.session_deletion_unresolved(reference) {
            return Err(conflict_error(
                "Session deletion outcome is unknown. Retry Delete before starting more work.",
            ));
        }
        Ok(())
    }

    /// Agent success precedes local removal. The adoption lock protects unowned identities;
    /// an adopted Task also uses the same lifecycle gate as Send, Archive and configuration.
    pub(super) fn delete_native_session(
        &self,
        client: &ClientInstanceId,
        params: NativeSessionDeleteParams,
        operation_id: &str,
    ) -> Result<NativeSessionDeleteResult, ProtocolError> {
        let started = std::time::Instant::now();
        let task_id = match &params.target {
            NativeSessionDeleteTarget::Task { task_id } => Some(task_id.clone()),
            _ => None,
        };
        crate::logging::info(
            "native_session_delete_started",
            serde_json::json!({
                "operation": "nativeSession/delete", "operation_id": operation_id,
                "task_id": task_id, "attempt": 1, "confirmed": params.confirmation.is_some(),
                "stage": "waiting_for_adoption_gate",
            }),
        );
        let result = (|| {
            let _adoption = self
                .native_adoption
                .lock()
                .map_err(|_| super::internal_error("Native Session mutation lock poisoned"))?;
            crate::logging::info(
                "native_session_delete_adoption_gate_acquired",
                serde_json::json!({
                    "operation_id": operation_id, "task_id": task_id,
                    "duration_ms": started.elapsed().as_millis(),
                }),
            );
            let target = self.resolve_delete_target(client, &params.target)?;
            if let Some(task) = &target.task {
                let waiting = std::time::Instant::now();
                crate::logging::info(
                    "native_session_delete_task_gate_waiting",
                    serde_json::json!({
                        "operation_id": operation_id, "task_id": task_id,
                    }),
                );
                self.session_operations.serialize(&task.task_id, || {
                    crate::logging::info(
                        "native_session_delete_task_gate_acquired",
                        serde_json::json!({
                            "operation_id": operation_id, "task_id": task_id,
                            "duration_ms": waiting.elapsed().as_millis(),
                        }),
                    );
                    let current = self.resolve_delete_target(client, &params.target)?;
                    self.delete_resolved_session(current, params, operation_id)
                })
            } else {
                self.delete_resolved_session(target, params, operation_id)
            }
        })();
        crate::logging::info(
            "native_session_delete_completed",
            serde_json::json!({
                "operation": "nativeSession/delete", "operation_id": operation_id,
                "task_id": task_id, "attempt": 1,
                "outcome": match &result { Ok(NativeSessionDeleteResult::Deleted { .. }) => "deleted", Ok(_) => "confirmation_required", Err(_) => "failure" },
                "duration_ms": started.elapsed().as_millis(),
                "error_kind": result.as_ref().err().map(|error| &error.code),
            }),
        );
        result
    }

    fn resolve_delete_target(
        &self,
        client: &ClientInstanceId,
        target: &NativeSessionDeleteTarget,
    ) -> Result<DeletionTarget, ProtocolError> {
        match target {
            NativeSessionDeleteTarget::Task { task_id } => {
                let task = self.read_task_for_client(task_id.as_str(), client)?;
                if matches!(task.lifecycle, TaskLifecycle::Prepared { .. }) {
                    return Err(conflict_error(
                        "Prepared Tasks must be discarded through New Task",
                    ));
                }
                let session_id = task
                    .agent_session_id
                    .as_ref()
                    .ok_or_else(|| conflict_error("This Task has no Native Session"))?;
                Ok(DeletionTarget {
                    reference: NativeSessionRef::new(&task.agent_id, session_id),
                    project_id: crate::projects::project_id_for_workspace(
                        task.project_root.as_deref().unwrap_or(&task.workspace_root),
                    ),
                    title: task
                        .title
                        .effective()
                        .map(|title| title.value().to_string())
                        .unwrap_or_else(|| "Untitled task".into()),
                    task: Some(task),
                })
            }
            NativeSessionDeleteTarget::NativeSession {
                agent_id,
                native_session_id,
            } => {
                let reference = NativeSessionRef::new(agent_id.as_str(), native_session_id);
                let entry = self.native_catalog.entry(&reference).ok_or_else(|| {
                    super::runtime_error(RuntimeError::TaskNotFound(
                        "Native Session no longer exists".into(),
                    ))
                })?;
                if self
                    .store
                    .list_all_task_records_strict()
                    .map_err(protocol_error_from_runtime)?
                    .iter()
                    .any(|task| {
                        !task.tombstoned
                            && task.agent_id == agent_id.as_str()
                            && task.agent_session_id.as_deref() == Some(native_session_id)
                    })
                {
                    return Err(conflict_error(
                        "This Native Session now belongs to a Task; delete it from that Task",
                    ));
                }
                Ok(DeletionTarget {
                    reference,
                    project_id: ProjectId::from(entry.project_id),
                    title: entry
                        .user_title
                        .or(entry.observation.title)
                        .or(entry.local_fallback_title)
                        .unwrap_or_else(|| "Untitled session".into()),
                    task: None,
                })
            }
        }
    }

    fn delete_resolved_session(
        &self,
        target: DeletionTarget,
        params: NativeSessionDeleteParams,
        operation_id: &str,
    ) -> Result<NativeSessionDeleteResult, ProtocolError> {
        crate::logging::info(
            "native_session_delete_target_resolved",
            serde_json::json!({
                "operation_id": operation_id, "agent_id": target.reference.agent_id,
                "session_id": target.reference.session_id,
            }),
        );
        let active = target.task.as_ref().is_some_and(|task| {
            task.active_turn_id.is_some()
                || matches!(
                    task.status,
                    TaskStatus::Starting
                        | TaskStatus::Active
                        | TaskStatus::Stopping
                        | TaskStatus::Waiting
                )
                || self
                    .server_requests
                    .has_pending_for_task(&TaskId::from(task.task_id.clone()))
        });
        let queued_message_count = target
            .task
            .as_ref()
            .map_or(0, |task| task.message_queue.items.len());
        if params.confirmation.as_ref().is_none_or(|confirmation| {
            (active && !confirmation.active)
                || confirmation.queued_message_count != queued_message_count
        }) {
            return Ok(NativeSessionDeleteResult::ConfirmationRequired {
                title: target.title,
                active,
                queued_message_count,
            });
        }
        // Preview uses local state so it cannot queue behind Agent history discovery.
        // Only a confirmed deletion needs a live capability check.
        let probe = self
            .agent_gateway
            .probe(AgentProbeRequest {
                agent_id: target.reference.agent_id.clone(),
            })
            .map_err(protocol_error_from_runtime)?;
        if !probe.typed_capabilities.delete_sessions {
            return Err(ProtocolError {
                code: ProtocolErrorCode::CapabilityUnavailable,
                message: "This Agent does not support session deletion".into(),
                recoverable: false,
                target: None,
            });
        }
        let first_attempt = self
            .turn_acceptance
            .begin_session_deletion(&target.reference);
        let deleted = self.agent_gateway.delete_session(AgentSessionDelete {
            agent_id: target.reference.agent_id.clone(),
            session_id: target.reference.session_id.clone(),
            operation_id: operation_id.to_string(),
        });
        if let Err(error) = deleted {
            if first_attempt && !matches!(error, RuntimeError::OutcomeUnknown(_)) {
                self.turn_acceptance
                    .resolve_session_deletion(&target.reference);
            }
            return Err(protocol_error_from_runtime(error));
        }
        self.remove_local_native_session(&target.reference, target.task.as_ref())
            .map_err(protocol_error_from_runtime)?;
        self.task_notifier
            .navigation_project_entries_changed(target.project_id.as_str().to_string());
        Ok(NativeSessionDeleteResult::Deleted {
            reference: NativeSessionReference {
                agent_id: AgentId::from(target.reference.agent_id),
                session_id: target.reference.session_id,
            },
            project_id: target.project_id,
            task_id: target.task.map(|task| TaskId::from(task.task_id)),
        })
    }

    /// Shared local cleanup after explicit Agent success or authoritative missing evidence.
    /// Referenced filesystem bytes are not owned by Task history and are never unlinked here.
    pub(super) fn remove_local_native_session(
        &self,
        reference: &NativeSessionRef,
        task: Option<&TaskRecord>,
    ) -> Result<(), RuntimeError> {
        self.remove_local_native_session_if_unchanged(reference, task, None, None)
            .map(|_| ())
    }

    pub(super) fn remove_local_native_session_if_unchanged(
        &self,
        reference: &NativeSessionRef,
        task: Option<&TaskRecord>,
        expected_revision: Option<u64>,
        observation_generation: Option<u64>,
    ) -> Result<bool, RuntimeError> {
        if let Some(task) = task {
            let commit = self.mutations.commit_existing_task(
                &task.task_id,
                TaskCommitOptions::metadata(),
                |ctx| {
                    if expected_revision.is_some_and(|revision| ctx.task().revision != revision) {
                        return Ok(TaskMutationResult::Unchanged);
                    }
                    if ctx.task().agent_id != reference.agent_id
                        || ctx.task().agent_session_id.as_deref() != Some(&reference.session_id)
                    {
                        return Err(RuntimeError::Conflict(
                            "Native Session binding changed during deletion".into(),
                        ));
                    }
                    if !self
                        .native_catalog
                        .remove_if_unobserved(reference, observation_generation)?
                    {
                        return Ok(TaskMutationResult::Unchanged);
                    }
                    let task = ctx.task_mut();
                    task.tombstoned = true;
                    task.status = TaskStatus::Inactive;
                    task.active_turn_id = None;
                    task.active_turn_started_at = None;
                    task.message_queue = Default::default();
                    task.composer_history = Default::default();
                    Ok(TaskMutationResult::Changed)
                },
            )?;
            if !matches!(commit.outcome, TaskCommitOutcome::Committed(_)) {
                return Ok(false);
            }
            self.server_requests.interrupt_task_requests(
                &TaskId::from(task.task_id.clone()),
                crate::client_lifecycle::AppServerTime::now(),
            );
            self.attachments
                .discard_resources_for_task(&TaskId::from(task.task_id.clone()));
            self.turn_acceptance.retire_for_idle_task(&task.task_id);
            if let Err(error) = self.mutations.purge_existing_tombstone(&task.task_id) {
                // The durable tombstone keeps it hidden; existing maintenance retries purge.
                crate::logging::warn(
                    "native_session_delete_local_cleanup_deferred",
                    serde_json::json!({ "task_id": task.task_id, "error_kind": error.reason() }),
                );
            }
        } else {
            if !self
                .native_catalog
                .remove_if_unobserved(reference, observation_generation)?
            {
                return Ok(false);
            }
        }
        self.turn_acceptance.resolve_session_deletion(reference);
        Ok(true)
    }
}
