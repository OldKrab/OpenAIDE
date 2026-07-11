use crate::agent::{AgentRuntime, AgentSessionDelete};
use crate::protocol::errors::RuntimeError;
use crate::storage::records::TaskRecord;

pub(crate) struct NativeSessionLifecycle<'a> {
    agent: &'a dyn AgentRuntime,
}

impl<'a> NativeSessionLifecycle<'a> {
    pub(crate) fn new(agent: &'a dyn AgentRuntime) -> Self {
        Self { agent }
    }

    pub(crate) fn delete_bound_session(&self, task: &TaskRecord) -> Result<(), RuntimeError> {
        let Some(session_id) = task.agent_session_id.clone() else {
            return Ok(());
        };
        self.agent.delete_session(AgentSessionDelete { session_id })
    }
}
