use std::collections::HashSet;

use openaide_app_server_protocol::errors::ProtocolError;

use crate::agent::AgentSessionKey;
use crate::native_sessions::catalog::{NativeSessionCatalogEntry, NativeSessionRef};
use crate::protocol::model::TaskStatus;
use crate::storage::records::{TaskLifecycle, TaskRecord};
use crate::tasks::task_operation::PassiveTaskOperation;

use super::{protocol_error_from_runtime, TaskProductApi};

/// Candidates and Task operation generations are captured before the first Agent request.
/// A later user operation or metadata revision invalidates removal of that Task.
pub(super) struct SessionReconciliationScan {
    pub(super) generation: u64,
    registry_revision: u64,
    tasks: Vec<(TaskRecord, PassiveTaskOperation)>,
    entries: Vec<NativeSessionCatalogEntry>,
}

impl TaskProductApi {
    pub(super) fn capture_session_reconciliation(
        &self,
        agent_id: &str,
        project_id: &str,
        workspace_root: &str,
        tasks: &[TaskRecord],
    ) -> SessionReconciliationScan {
        SessionReconciliationScan {
            generation: self.native_catalog.observation_generation(),
            registry_revision: self.agent_registry.revision(),
            tasks: tasks
                .iter()
                .filter(|task| {
                    !task.tombstoned
                        && task.agent_id == agent_id
                        && task.workspace_root == workspace_root
                        && !matches!(task.lifecycle, TaskLifecycle::Prepared { .. })
                        && task.agent_session_id.is_some()
                        // Live work can precede the Agent's durable history entry.
                        // Defer listing-absence cleanup until its turn has settled.
                        && task.active_turn_id.is_none()
                        && !matches!(task.status, TaskStatus::Starting | TaskStatus::Active | TaskStatus::Waiting | TaskStatus::Stopping)
                })
                .map(|task| {
                    (
                        task.clone(),
                        self.session_operations.begin_passive(&task.task_id),
                    )
                })
                .collect(),
            entries: self
                .native_catalog
                .entries()
                .into_iter()
                .filter(|entry| {
                    entry.project_id == project_id
                        && entry.workspace_root == workspace_root
                        && entry.observation.reference.agent_id == agent_id
                })
                .collect(),
        }
    }

    /// Call only on genuine provider exhaustion. This consumes existing observations;
    /// neither an early stop nor a failed page is evidence that history disappeared.
    pub(super) fn reconcile_completed_session_scan(
        &self,
        scan: SessionReconciliationScan,
        seen: &HashSet<String>,
        project_id: &str,
    ) -> Result<(), ProtocolError> {
        let _adoption = self
            .native_adoption
            .lock()
            .map_err(|_| super::internal_error("Native Session mutation lock poisoned"))?;
        let mut removed_count = 0;
        for (task, operation) in scan.tasks {
            let session_id = task.agent_session_id.as_ref().expect("captured bound Task");
            if seen.contains(session_id) {
                continue;
            }
            let reference = NativeSessionRef::new(&task.agent_id, session_id);
            let result = self
                .session_operations
                .try_serialize_passive(&operation, || {
                    self.agent_registry
                        .with_revision(scan.registry_revision, || {
                            self.remove_local_native_session_if_unchanged(
                                &reference,
                                Some(&task),
                                Some(task.revision),
                                Some(scan.generation),
                            )
                        })
                        .unwrap_or(Ok(false))
                });
            if let Some(result) = result {
                if result.map_err(protocol_error_from_runtime)? {
                    removed_count += 1;
                    self.release_removed_session(&reference);
                }
            }
        }
        let owned = self
            .store
            .list_all_task_records_strict()
            .map_err(protocol_error_from_runtime)?
            .into_iter()
            .filter(|task| !task.tombstoned)
            .filter_map(|task| {
                task.agent_session_id
                    .map(|id| NativeSessionRef::new(task.agent_id, id))
            })
            .collect::<HashSet<_>>();
        for entry in scan.entries {
            let reference = &entry.observation.reference;
            if seen.contains(&reference.session_id) || owned.contains(reference) {
                continue;
            }
            if self.native_catalog.entry(reference).as_ref() != Some(&entry) {
                continue;
            }
            if self
                .agent_registry
                .with_revision(scan.registry_revision, || {
                    self.remove_local_native_session_if_unchanged(
                        reference,
                        None,
                        None,
                        Some(scan.generation),
                    )
                })
                .unwrap_or(Ok(false))
                .map_err(protocol_error_from_runtime)?
            {
                removed_count += 1;
            }
        }
        if removed_count > 0 {
            self.task_notifier
                .navigation_project_entries_changed(project_id.to_string());
        }
        crate::logging::info(
            "native_session_history_reconciled",
            serde_json::json!({
                "project_id": project_id, "removed_count": removed_count, "evidence": "complete_listing",
            }),
        );
        Ok(())
    }

    pub(super) fn task_history_removed(&self, task_id: &str) -> bool {
        self.store.read_task(task_id).map_or_else(
            |error| {
                matches!(
                    error,
                    crate::protocol::errors::RuntimeError::TaskNotFound(_)
                )
            },
            |task| task.tombstoned,
        )
    }

    fn release_removed_session(&self, reference: &NativeSessionRef) {
        if let Err(error) = self.agent_gateway.close_session(&AgentSessionKey::new(
            &reference.agent_id,
            &reference.session_id,
        )) {
            crate::logging::warn(
                "missing_native_session_cleanup_failed",
                serde_json::json!({
                    "agent_id": reference.agent_id, "session_id": reference.session_id, "error_kind": error.reason(),
                }),
            );
        }
    }
}

impl TaskProductApi {
    /// Called while holding the Task session gate. Only typed, definitive Agent absence
    /// permits removal; newer positive catalog evidence still supersedes this request.
    pub(super) fn reconcile_session_recovery_error(
        &self,
        task: &TaskRecord,
        generation: u64,
        error: crate::protocol::errors::RuntimeError,
    ) -> ProtocolError {
        if matches!(
            error,
            crate::protocol::errors::RuntimeError::NativeSessionMissing(_)
        ) {
            if let Some(session_id) = &task.agent_session_id {
                let reference = NativeSessionRef::new(&task.agent_id, session_id);
                match self.remove_local_native_session_if_unchanged(
                    &reference,
                    Some(task),
                    None,
                    Some(generation),
                ) {
                    Ok(true) => {
                        self.release_removed_session(&reference);
                        self.task_notifier.navigation_project_entries_changed(
                            crate::projects::project_id_for_workspace(
                                task.project_root.as_deref().unwrap_or(&task.workspace_root),
                            )
                            .as_str()
                            .to_string(),
                        );
                    }
                    Ok(false) => {}
                    Err(cleanup_error) => return protocol_error_from_runtime(cleanup_error),
                }
            }
        }
        protocol_error_from_runtime(error)
    }
}
