use crate::agent::gateway::AgentGateway;
use crate::agent::AgentSession;
use crate::protocol::errors::RuntimeError;

pub(crate) struct TaskSessionStartGuard<'a> {
    agent_gateway: &'a AgentGateway,
    session: Option<AgentSession>,
}

impl<'a> TaskSessionStartGuard<'a> {
    pub(crate) fn new(agent_gateway: &'a AgentGateway, session: AgentSession) -> Self {
        Self {
            agent_gateway,
            session: Some(session),
        }
    }

    pub(crate) fn session(&self) -> &AgentSession {
        self.session
            .as_ref()
            .expect("task session start guard consumed")
    }

    pub(crate) fn session_id(&self) -> &str {
        &self.session().session_id
    }

    pub(crate) fn close(&mut self) -> Result<(), RuntimeError> {
        if let Some(session) = self.session.take() {
            self.agent_gateway.close_session(&session.session_id)?;
        }
        Ok(())
    }

    pub(crate) fn commit(mut self) -> AgentSession {
        self.session
            .take()
            .expect("task session start guard consumed")
    }
}

impl Drop for TaskSessionStartGuard<'_> {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
