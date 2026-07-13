use super::*;
use crate::agent::acp_host_terminal_ownership::{AcpHostTerminalRegistry, AcpTerminalOwnerId};
use std::sync::mpsc;
use tokio::sync::mpsc as tokio_mpsc;

fn stopped_client() -> AcpSessionClient {
    let (command_tx, _command_rx) = tokio_mpsc::unbounded_channel();
    let (config_tx, _config_rx) = tokio_mpsc::unbounded_channel();
    let (cancel_tx, _cancel_rx) = tokio_mpsc::unbounded_channel();
    let (close_tx, _close_rx) =
        tokio_mpsc::unbounded_channel::<mpsc::Sender<Result<(), RuntimeError>>>();
    let terminal_error = Arc::new(Mutex::new(Some("Agent process stopped".to_string())));
    let terminals = AcpHostTerminalRegistry::new(crate::protocol::host::HostBridge::disabled());
    let owner_id = AcpTerminalOwnerId::next();
    terminals.begin_open(owner_id);

    AcpSessionClient::new(
        command_tx,
        config_tx,
        cancel_tx,
        close_tx,
        terminal_error,
        terminals.owner(owner_id),
    )
}

#[test]
fn stopped_session_is_not_reported_as_active() {
    let registry = AcpActiveSessionRegistry::new();
    let key = AgentSessionKey::new("agent", "session");
    registry
        .insert_started_session(key.clone(), stopped_client())
        .expect("insert session");

    assert!(!registry.contains(&key));
    assert!(registry.get(&key).is_none());
}
