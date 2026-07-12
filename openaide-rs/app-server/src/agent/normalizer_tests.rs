use crate::agent::events::AgentEvent;
use crate::agent::normalizer::normalize_events;
use crate::protocol::model::{AgentCommandsCatalog, ConfigOptionsCatalog};

#[test]
fn session_catalog_updates_do_not_create_chat_messages() {
    let messages = normalize_events(
        vec![
            AgentEvent::CommandsChanged(AgentCommandsCatalog::default()),
            AgentEvent::ConfigOptionsChanged(ConfigOptionsCatalog::empty("codex")),
        ],
        "2026-07-07T00:00:00.000Z",
    );

    assert!(messages.is_empty());
}
