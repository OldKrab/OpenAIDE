use crate::protocol::errors::RuntimeError;
use crate::storage::id::validate_task_id;
use crate::storage::records::TaskRecord;

use super::{
    artifact, recovery, RecoveredTask, TaskJournalStore, TaskProjection, ToolArtifactProjection,
};

/// Read side of the store: committed projections and Tool artifacts, never a write.
impl TaskJournalStore {
    pub fn load(&self, task_id: &str) -> Result<TaskProjection, RuntimeError> {
        #[cfg(test)]
        self.inner
            .projection_clones
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        validate_task_id(task_id)?;
        recovery::ensure_task_loaded(
            &self.inner.tasks_root,
            &self.inner.catalog,
            &self.inner.epoch_task_overlays,
            &self.inner.projections,
            &self.inner.projection_load_lock,
            task_id,
        )?;
        self.inner
            .projections
            .read()
            .expect("Task journal projections poisoned")
            .get(task_id)
            .map(|task| match task {
                RecoveredTask::Available { projection, .. } => Ok(projection.as_ref().clone()),
                RecoveredTask::Unavailable { error } => Err(RuntimeError::Storage(error.clone())),
            })
            .transpose()?
            .ok_or_else(|| RuntimeError::TaskNotFound(task_id.to_string()))
    }

    /// Projects metadata without cloning Chat or queued attachment payloads. The
    /// callback runs under a read lock and must not call back into storage.
    pub(crate) fn inspect_task_record<T>(
        &self,
        task_id: &str,
        inspect: impl FnOnce(&TaskRecord) -> T,
    ) -> Result<T, RuntimeError> {
        self.inspect_projection(task_id, |projection| inspect(&projection.task))
    }

    /// Reads a committed projection in place. Chat history makes a clone expensive on a long
    /// Task, and these reads can run once per Tool row of a Chat page. The callback runs under a
    /// read lock and must not call back into storage.
    fn inspect_projection<T>(
        &self,
        task_id: &str,
        inspect: impl FnOnce(&TaskProjection) -> T,
    ) -> Result<T, RuntimeError> {
        validate_task_id(task_id)?;
        recovery::ensure_task_loaded(
            &self.inner.tasks_root,
            &self.inner.catalog,
            &self.inner.epoch_task_overlays,
            &self.inner.projections,
            &self.inner.projection_load_lock,
            task_id,
        )?;
        self.inner
            .projections
            .read()
            .expect("Task journal projections poisoned")
            .get(task_id)
            .map(|task| match task {
                RecoveredTask::Available { projection, .. } => Ok(inspect(projection)),
                RecoveredTask::Unavailable { error } => Err(RuntimeError::Storage(error.clone())),
            })
            .transpose()?
            .ok_or_else(|| RuntimeError::TaskNotFound(task_id.to_string()))
    }

    /// Returns lightweight Task records without hydrating Chat or Tool details.
    pub fn list_task_records(&self) -> Vec<TaskRecord> {
        self.inner
            .catalog
            .read()
            .expect("Task catalog poisoned")
            .values()
            .cloned()
            .collect()
    }

    /// Strict collection reads preserve fail-closed behavior for Tasks whose
    /// journal had to be recovered while rebuilding a missing catalog.
    pub fn list_task_records_strict(&self) -> Result<Vec<TaskRecord>, RuntimeError> {
        if let Some(error) = self
            .inner
            .projections
            .read()
            .expect("Task journal projections poisoned")
            .values()
            .find_map(|task| match task {
                RecoveredTask::Unavailable { error } => Some(error.clone()),
                RecoveredTask::Available { .. } => None,
            })
        {
            return Err(RuntimeError::Storage(error));
        }
        Ok(self.list_task_records())
    }

    /// Loads Tool detail only when requested, bounded by the artifact head
    /// durably referenced from the Task journal.
    pub fn load_tool_artifact(
        &self,
        task_id: &str,
        artifact_id: &str,
    ) -> Result<ToolArtifactProjection, RuntimeError> {
        validate_task_id(task_id)?;
        artifact::validate_artifact_id(artifact_id)?;
        let committed_head = self
            .inspect_projection(task_id, |projection| {
                projection.artifact_heads.get(artifact_id).copied()
            })?
            .ok_or_else(|| {
                RuntimeError::Storage(format!("Tool artifact is not committed: {artifact_id}"))
            })?;
        let artifact_key = (task_id.to_string(), artifact_id.to_string());
        let mut heads = self
            .inner
            .artifact_heads
            .lock()
            .expect("Tool artifact reconciliation lock poisoned");
        if let std::collections::hash_map::Entry::Vacant(entry) = heads.entry(artifact_key) {
            let reconciled = artifact::reconcile_one(
                &self.inner.tasks_root,
                task_id,
                artifact_id,
                committed_head,
            )?;
            entry.insert(reconciled);
        }
        let mut artifact =
            artifact::load(&self.inner.tasks_root, task_id, artifact_id, committed_head)?;
        artifact.revision = committed_head;
        Ok(artifact)
    }
}
