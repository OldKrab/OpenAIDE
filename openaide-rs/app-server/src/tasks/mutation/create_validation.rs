use crate::protocol::errors::RuntimeError;
use crate::storage::Store;

use super::TaskMutations;

pub(crate) struct TaskCreationValidationContext<'a> {
    store: &'a Store,
}

impl<'a> TaskCreationValidationContext<'a> {
    pub(super) fn new(store: &'a Store) -> Self {
        Self { store }
    }

    pub(crate) fn ensure_native_session_unowned(
        &self,
        agent_id: &str,
        session_id: &str,
    ) -> Result<(), RuntimeError> {
        ensure_native_session_unowned(self.store, agent_id, session_id)
    }
}

impl TaskMutations {
    pub(crate) fn ensure_native_session_unowned(
        &self,
        agent_id: &str,
        session_id: &str,
    ) -> Result<(), RuntimeError> {
        let _guard = self.lock();
        ensure_native_session_unowned(self.store(), agent_id, session_id)
    }
}

fn ensure_native_session_unowned(
    store: &Store,
    agent_id: &str,
    session_id: &str,
) -> Result<(), RuntimeError> {
    let records = store.list_all_task_records_strict()?;
    if let Some(owner) = records.into_iter().find(|record| {
        record.agent_id == agent_id && record.agent_session_id.as_deref() == Some(session_id)
    }) {
        return Err(RuntimeError::InvalidParams(format!(
            "external_session_id already adopted by {}",
            owner.task_id
        )));
    }
    Ok(())
}
