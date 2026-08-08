use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::agent::attached_native_session::AttachedNativeSession;
use crate::agent::{
    AgentEventSink, AgentLoadedSession, AgentPrompt, AgentPromptOutcome, AgentSession,
    AgentSessionDelete, AgentSessionEventSink, AgentSessionKey, AgentSessionLoad,
};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{ConfigOptionCurrentValue, ConfigOptionsCatalog};

pub(super) struct AttachedNativeSessionRegistry {
    sessions: Mutex<HashMap<AgentSessionKey, AttachedNativeSession>>,
    /// Task sinks outlive one active attachment so resume can restore live updates.
    session_event_sinks: Mutex<HashMap<AgentSessionKey, Arc<dyn AgentSessionEventSink>>>,
}

impl AttachedNativeSessionRegistry {
    pub(super) fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            session_event_sinks: Mutex::new(HashMap::new()),
        }
    }

    pub(super) fn contains(&self, session: &AgentSessionKey) -> bool {
        self.get(session).is_some()
    }

    pub(super) fn insert_started_session(
        &self,
        session: AgentSessionKey,
        session_attachment: AttachedNativeSession,
    ) -> Result<(), RuntimeError> {
        let mut sessions = self.sessions.lock().expect("ACP session registry poisoned");
        // Idle attachments close their receivers independently. Pruning before every
        // insertion keeps dead handles from accumulating as Tasks are opened.
        sessions.retain(|_, attachment| attachment.is_running());
        if sessions.contains_key(&session) {
            drop(sessions);
            let _ = session_attachment.close();
            return Err(RuntimeError::InvalidParams(
                "agent_session_id already active".to_string(),
            ));
        }

        let retained_sink = self
            .session_event_sinks
            .lock()
            .expect("ACP session sink registry poisoned")
            .get(&session)
            .cloned();
        if let Some(sink) = retained_sink {
            if let Err(error) = session_attachment.set_event_sink(sink) {
                drop(sessions);
                let _ = session_attachment.close();
                return Err(error);
            }
        }

        sessions.insert(session, session_attachment);
        Ok(())
    }

    pub(super) fn attach_session_event_sink(
        &self,
        session: &AgentSessionKey,
        sink: Arc<dyn AgentSessionEventSink>,
    ) -> Result<(), RuntimeError> {
        self.require_session(session)?
            .set_event_sink(sink.clone())?;
        self.session_event_sinks
            .lock()
            .expect("ACP session sink registry poisoned")
            .insert(session.clone(), sink);
        Ok(())
    }

    pub(super) fn load_session(
        &self,
        session: &AgentSessionKey,
        request: AgentSessionLoad,
    ) -> Result<AgentLoadedSession, RuntimeError> {
        self.require_session(session)?.load_session(request)
    }

    /// Reads the state held by an already attached Native Session without
    /// issuing another ACP lifecycle request.
    pub(super) fn snapshot_session(
        &self,
        session: &AgentSessionKey,
    ) -> Result<AgentSession, RuntimeError> {
        self.require_session(session)?.snapshot()
    }

    pub(super) fn set_config_option(
        &self,
        session: &AgentSessionKey,
        config_id: String,
        value: ConfigOptionCurrentValue,
        operation_id: String,
    ) -> Result<ConfigOptionsCatalog, RuntimeError> {
        self.require_session(session)?.set_config_option(
            session.agent_id().to_string(),
            session.session_id().to_string(),
            config_id,
            value,
            operation_id,
        )
    }

    pub(super) fn prompt(
        &self,
        prompt: AgentPrompt,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<AgentPromptOutcome, RuntimeError> {
        let session = prompt.session_key();
        self.require_session(&session)?.prompt(prompt, sink)
    }

    pub(super) fn steer(&self, prompt: AgentPrompt) -> Result<(), RuntimeError> {
        let session = prompt.session_key();
        self.require_session(&session)?.steer(prompt)
    }

    pub(super) fn cancel_session(&self, session: &AgentSessionKey) -> Result<(), RuntimeError> {
        if let Some(attachment) = self.get(session) {
            attachment.cancel()?;
        }
        Ok(())
    }

    pub(super) fn close_session(&self, session: &AgentSessionKey) -> Result<(), RuntimeError> {
        self.remove_event_sink(session);
        if let Some(attachment) = self.remove(session) {
            attachment.close()?;
        }
        Ok(())
    }

    pub(super) fn delete_session(&self, request: AgentSessionDelete) -> Result<(), RuntimeError> {
        let key = request.session_key();
        self.remove_event_sink(&key);
        let attachment = self.remove(&key).ok_or_else(not_ready)?;
        attachment.delete()
    }

    pub(super) fn take_shutdown_close_tasks(&self) -> Vec<Box<dyn FnOnce() + Send + 'static>> {
        self.session_event_sinks
            .lock()
            .expect("ACP session sink registry poisoned")
            .clear();
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

    fn require_session(
        &self,
        session: &AgentSessionKey,
    ) -> Result<AttachedNativeSession, RuntimeError> {
        self.get(session).ok_or_else(not_ready)
    }

    fn get(&self, session: &AgentSessionKey) -> Option<AttachedNativeSession> {
        let mut sessions = self.sessions.lock().expect("ACP session registry poisoned");
        if sessions
            .get(session)
            .is_some_and(AttachedNativeSession::is_running)
        {
            return sessions.get(session).cloned();
        }

        // Dead handles must not make resume succeed. The next explicit Send can then
        // load the Native Session through the normal session service path.
        let removed = sessions.remove(session).is_some();
        drop(sessions);
        if removed {
            crate::logging::warn(
                "acp_dead_session_handle_evicted",
                serde_json::json!({
                    "agent_id": session.agent_id(),
                    "session_id": session.session_id(),
                }),
            );
        }
        None
    }

    fn remove(&self, session: &AgentSessionKey) -> Option<AttachedNativeSession> {
        self.sessions
            .lock()
            .expect("ACP session registry poisoned")
            .remove(session)
    }

    fn remove_event_sink(&self, session: &AgentSessionKey) {
        self.session_event_sinks
            .lock()
            .expect("ACP session sink registry poisoned")
            .remove(session);
    }
}

fn not_ready() -> RuntimeError {
    RuntimeError::NotReady("ACP session is not active".to_string())
}

#[cfg(test)]
#[path = "attached_native_session_registry_tests.rs"]
mod tests;
