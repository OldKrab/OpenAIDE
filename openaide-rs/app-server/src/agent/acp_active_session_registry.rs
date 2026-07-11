use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::agent::acp_session_client::AcpSessionClient;
use crate::agent::{AgentEventSink, AgentPrompt, AgentSessionDelete, AgentSessionEventSink};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::ConfigOptionsCatalog;

pub(super) struct AcpActiveSessionRegistry {
    sessions: Mutex<HashMap<String, AcpSessionClient>>,
}

impl AcpActiveSessionRegistry {
    pub(super) fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub(super) fn contains(&self, session_id: &str) -> bool {
        self.sessions
            .lock()
            .expect("ACP session registry poisoned")
            .contains_key(session_id)
    }

    pub(super) fn insert_started_session(
        &self,
        session_id: &str,
        session_client: AcpSessionClient,
    ) -> Result<(), RuntimeError> {
        let mut sessions = self.sessions.lock().expect("ACP session registry poisoned");
        if sessions.contains_key(session_id) {
            drop(sessions);
            let _ = session_client.close();
            return Err(RuntimeError::InvalidParams(
                "agent_session_id already active".to_string(),
            ));
        }

        sessions.insert(session_id.to_string(), session_client);
        Ok(())
    }

    pub(super) fn attach_session_event_sink(
        &self,
        session_id: &str,
        sink: Arc<dyn AgentSessionEventSink>,
    ) -> Result<(), RuntimeError> {
        self.require_session(session_id)?.set_event_sink(sink)
    }

    pub(super) fn set_config_option(
        &self,
        session_id: &str,
        agent_id: String,
        config_id: String,
        value: String,
    ) -> Result<ConfigOptionsCatalog, RuntimeError> {
        self.require_session(session_id)?
            .set_config_option(agent_id, config_id, value)
    }

    pub(super) fn prompt(
        &self,
        prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<(), RuntimeError> {
        self.require_session(&prompt.session_id)?
            .prompt(prompt, sink)
    }

    pub(super) fn cancel_session(&self, session_id: &str) -> Result<(), RuntimeError> {
        if let Some(session) = self.get(session_id) {
            session.cancel()?;
        }
        Ok(())
    }

    pub(super) fn close_session(&self, session_id: &str) -> Result<(), RuntimeError> {
        if let Some(session) = self.remove(session_id) {
            session.close()?;
        }
        Ok(())
    }

    pub(super) fn delete_session(&self, request: AgentSessionDelete) -> Result<(), RuntimeError> {
        let session = self.remove(&request.session_id).ok_or_else(not_ready)?;
        session.delete()
    }

    pub(super) fn take_shutdown_close_tasks(&self) -> Vec<Box<dyn FnOnce() + Send + 'static>> {
        let sessions =
            std::mem::take(&mut *self.sessions.lock().expect("ACP session registry poisoned"));
        sessions
            .into_values()
            .map(|session| {
                Box::new(move || {
                    let _ = session.close();
                }) as Box<dyn FnOnce() + Send + 'static>
            })
            .collect()
    }

    fn require_session(&self, session_id: &str) -> Result<AcpSessionClient, RuntimeError> {
        self.get(session_id).ok_or_else(not_ready)
    }

    fn get(&self, session_id: &str) -> Option<AcpSessionClient> {
        self.sessions
            .lock()
            .expect("ACP session registry poisoned")
            .get(session_id)
            .cloned()
    }

    fn remove(&self, session_id: &str) -> Option<AcpSessionClient> {
        self.sessions
            .lock()
            .expect("ACP session registry poisoned")
            .remove(session_id)
    }
}

fn not_ready() -> RuntimeError {
    RuntimeError::NotReady("ACP session is not active".to_string())
}
