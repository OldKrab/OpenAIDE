use super::*;

#[test]
fn newer_stream_supersedes_old_stream_without_losing_registration() {
    let registry = EventStreamRegistry::default();
    let connection_id = ConnectionId::new("local-http:client-1");
    let old = registry.begin(connection_id.clone());
    let current = registry.begin(connection_id.clone());

    assert!(!registry.is_current(&old));
    assert!(registry.is_current(&current));
    registry.finish(&old);
    assert!(registry.is_active(&connection_id));
    registry.finish(&current);
    assert!(!registry.is_active(&connection_id));
}
