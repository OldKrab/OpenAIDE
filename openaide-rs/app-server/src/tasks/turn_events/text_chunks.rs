use std::sync::Mutex;

use uuid::Uuid;

use crate::agent::acp_message_identity::stable_message_id;

#[derive(Clone, Copy)]
pub(super) enum TextChannel {
    Agent,
    Thought,
}

/// Correlates only legacy anonymous chunks; sourced ACP identity needs no runtime lookup state.
pub(super) struct TextChunkRoutes {
    session_id: String,
    anonymous: Mutex<AnonymousRoutes>,
}

#[derive(Default)]
struct AnonymousRoutes {
    agent: Option<String>,
    thought: Option<String>,
}

impl TextChunkRoutes {
    pub(super) fn new(session_id: String) -> Self {
        Self {
            session_id,
            anonymous: Mutex::default(),
        }
    }

    pub(super) fn message_id(
        &self,
        channel: TextChannel,
        source_message_id: Option<String>,
    ) -> String {
        if let Some(source_message_id) = source_message_id {
            self.finish_anonymous(channel);
            return stable_message_id(&self.session_id, &source_message_id);
        }

        let mut anonymous = self.anonymous.lock().expect("text route lock poisoned");
        let route = match channel {
            TextChannel::Agent => &mut anonymous.agent,
            TextChannel::Thought => &mut anonymous.thought,
        };
        route
            .get_or_insert_with(|| Uuid::new_v4().to_string())
            .clone()
    }

    pub(super) fn finish_anonymous(&self, channel: TextChannel) {
        let mut anonymous = self.anonymous.lock().expect("text route lock poisoned");
        match channel {
            TextChannel::Agent => anonymous.agent = None,
            TextChannel::Thought => anonymous.thought = None,
        }
    }

    pub(super) fn finish_all_anonymous(&self) {
        *self.anonymous.lock().expect("text route lock poisoned") = AnonymousRoutes::default();
    }
}
