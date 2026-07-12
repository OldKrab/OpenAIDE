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

#[test]
fn superseded_stream_cannot_drain_after_replacement_becomes_current() {
    let registry = EventStreamRegistry::default();
    let connection_id = ConnectionId::new("local-http:client-1");
    let old = registry.begin(connection_id.clone());
    let current = registry.begin(connection_id);
    let mut old_drain_ran = false;
    let mut current_drain_ran = false;

    let old_result = registry.with_current(&old, || {
        old_drain_ran = true;
        "old"
    });
    let current_result = registry.with_current(&current, || {
        current_drain_ran = true;
        "current"
    });

    assert_eq!(old_result, None);
    assert!(!old_drain_ran);
    assert_eq!(current_result, Some("current"));
    assert!(current_drain_ran);
}
