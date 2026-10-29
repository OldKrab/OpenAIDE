use super::*;

use crate::protocol::model::ConfigOptionsStatus;

fn ready_catalog() -> ConfigOptionsCatalog {
    ConfigOptionsCatalog {
        status: ConfigOptionsStatus::Ready,
        ..ConfigOptionsCatalog::empty("codex")
    }
}

#[test]
fn reader_sees_the_started_session_and_later_catalogs() {
    let (publisher, reader) = session_snapshot_channel();
    let session = AgentSession::new("codex", "session");
    publisher.publish(&session);
    assert_eq!(
        reader.read().map(|session| session.session_id),
        Some("session".to_string())
    );

    let commands = Some(AgentCommandsCatalog::default());
    publisher.publish_catalogs(&ready_catalog(), &commands);
    let read = reader.read().expect("snapshot");
    assert_eq!(read.config_catalog, Some(ready_catalog()));
    assert_eq!(read.commands_catalog, commands);
}

#[test]
fn unreported_config_catalog_keeps_the_published_one() {
    let (publisher, reader) = session_snapshot_channel();
    let session = AgentSession::new("codex", "session").with_config_options(&ready_catalog());
    publisher.publish(&session);

    publisher.publish_catalogs(&ConfigOptionsCatalog::empty(""), &None);

    assert_eq!(
        reader.read().and_then(|session| session.config_catalog),
        Some(ready_catalog())
    );
}

#[test]
fn reader_reports_a_stopped_event_loop() {
    let (publisher, reader) = session_snapshot_channel();
    publisher.publish(&AgentSession::new("codex", "session"));
    drop(publisher);

    assert!(reader.read().is_none());
}
