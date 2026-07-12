use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::agent::acp_session_client::AcpSessionClient;
use crate::agent::{
    AgentEventSink, AgentPrompt, AgentSessionDelete, AgentSessionEventSink, AgentSessionKey,
};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::ConfigOptionsCatalog;

pub(super) struct AcpActiveSessionRegistry {
    sessions: Mutex<HashMap<AgentSessionKey, AcpSessionClient>>,
}

impl AcpActiveSessionRegistry {
    pub(super) fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub(super) fn contains(&self, session: &AgentSessionKey) -> bool {
        self.sessions
            .lock()
            .expect("ACP session registry poisoned")
            .contains_key(session)
    }

    pub(super) fn insert_started_session(
        &self,
        session: AgentSessionKey,
        session_client: AcpSessionClient,
    ) -> Result<(), RuntimeError> {
        let mut sessions = self.sessions.lock().expect("ACP session registry poisoned");
        if sessions.contains_key(&session) {
            drop(sessions);
            let _ = session_client.close();
            return Err(RuntimeError::InvalidParams(
                "agent_session_id already active".to_string(),
            ));
        }

        sessions.insert(session, session_client);
        Ok(())
    }

    pub(super) fn attach_session_event_sink(
        &self,
        session: &AgentSessionKey,
        sink: Arc<dyn AgentSessionEventSink>,
    ) -> Result<(), RuntimeError> {
        self.require_session(session)?.set_event_sink(sink)
    }

    pub(super) fn set_config_option(
        &self,
        session: &AgentSessionKey,
        config_id: String,
        value: String,
    ) -> Result<ConfigOptionsCatalog, RuntimeError> {
        self.require_session(session)?.set_config_option(
            session.agent_id().to_string(),
            config_id,
            value,
        )
    }

    pub(super) fn prompt(
        &self,
        prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<(), RuntimeError> {
        let session = prompt.session_key();
        self.require_session(&session)?.prompt(prompt, sink)
    }

    pub(super) fn cancel_session(&self, session: &AgentSessionKey) -> Result<(), RuntimeError> {
        if let Some(client) = self.get(session) {
            client.cancel()?;
        }
        Ok(())
    }

    pub(super) fn close_session(&self, session: &AgentSessionKey) -> Result<(), RuntimeError> {
        if let Some(client) = self.remove(session) {
            client.close()?;
        }
        Ok(())
    }

    pub(super) fn delete_session(&self, request: AgentSessionDelete) -> Result<(), RuntimeError> {
        let key = request.session_key();
        let client = self.remove(&key).ok_or_else(not_ready)?;
        client.delete()
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

    fn require_session(&self, session: &AgentSessionKey) -> Result<AcpSessionClient, RuntimeError> {
        self.get(session).ok_or_else(not_ready)
    }

    fn get(&self, session: &AgentSessionKey) -> Option<AcpSessionClient> {
        self.sessions
            .lock()
            .expect("ACP session registry poisoned")
            .get(session)
            .cloned()
    }

    fn remove(&self, session: &AgentSessionKey) -> Option<AcpSessionClient> {
        self.sessions
            .lock()
            .expect("ACP session registry poisoned")
            .remove(session)
    }
}

fn not_ready() -> RuntimeError {
    RuntimeError::NotReady("ACP session is not active".to_string())
}
