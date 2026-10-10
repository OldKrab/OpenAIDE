use super::*;

#[test]
fn only_a_session_being_opened_may_defer_its_catalog() {
    let catalogs = EarlySessionCatalogs::default();
    assert!(!catalogs.defer_commands("session_1"));

    let open = catalogs.begin_open();
    assert!(catalogs.defer_commands("session_1"));
    open.finish("session_1");

    assert!(!catalogs.defer_commands("session_1"));
}

#[test]
fn unclaimed_updates_exhaust_the_connection_budget() {
    let catalogs = EarlySessionCatalogs::default();
    for _ in 0..UNCLAIMED_UPDATE_LIMIT {
        let open = catalogs.begin_open();
        assert!(catalogs.defer_commands("never_attached"));
        drop(open);
    }

    let open = catalogs.begin_open();
    assert!(!catalogs.defer_commands("session_1"));
    drop(open);
}

#[test]
fn claimed_updates_do_not_spend_the_budget() {
    let catalogs = EarlySessionCatalogs::default();
    for _ in 0..UNCLAIMED_UPDATE_LIMIT * 2 {
        let open = catalogs.begin_open();
        assert!(catalogs.defer_commands("session_1"));
        open.finish("session_1");
    }
}
