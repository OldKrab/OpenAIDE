use crate::storage::Store;

use super::catalog::{NativeSessionCatalog, NativeSessionObservation, NativeSessionRef};

#[test]
fn listed_native_sessions_survive_app_server_restart() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let catalog = NativeSessionCatalog::open(store).unwrap();
    let reference = NativeSessionRef::new("codex", "session-1");

    catalog
        .record_page(
            "project-1",
            "/workspace/project",
            vec![NativeSessionObservation {
                reference: reference.clone(),
                title: Some("Persist me".to_string()),
                last_activity: Some("2026-07-21T12:00:00Z".to_string()),
            }],
        )
        .unwrap();
    drop(catalog);

    let reopened =
        NativeSessionCatalog::open(Store::open(temp.path().to_path_buf()).unwrap()).unwrap();

    assert_eq!(
        reopened.project("project-1"),
        vec![NativeSessionObservation {
            reference,
            title: Some("Persist me".to_string()),
            last_activity: Some("2026-07-21T12:00:00Z".to_string()),
        }]
    );
}

#[test]
fn definitive_load_failure_removal_survives_restart() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let catalog = NativeSessionCatalog::open(store).unwrap();
    let reference = NativeSessionRef::new("codex", "gone-session");
    catalog
        .record_page(
            "project-1",
            "/workspace/project",
            vec![NativeSessionObservation {
                reference: reference.clone(),
                title: None,
                last_activity: None,
            }],
        )
        .unwrap();

    assert!(catalog.remove(&reference).unwrap());
    drop(catalog);

    let reopened =
        NativeSessionCatalog::open(Store::open(temp.path().to_path_buf()).unwrap()).unwrap();
    assert!(reopened.entries().is_empty());
}

#[test]
fn stale_listing_cannot_replace_newer_live_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let catalog = NativeSessionCatalog::open(store).unwrap();
    let reference = NativeSessionRef::new("codex", "session-live");
    catalog
        .record_page(
            "project-1",
            "/workspace/project",
            vec![NativeSessionObservation {
                reference: reference.clone(),
                title: Some("Listed title".to_string()),
                last_activity: Some("2026-07-21T12:00:00Z".to_string()),
            }],
        )
        .unwrap();

    catalog
        .record_live_metadata(
            &reference,
            Some(Some("Live title".to_string())),
            Some("2026-07-21T13:00:00Z".to_string()),
        )
        .unwrap();
    catalog
        .record_page(
            "project-1",
            "/workspace/project",
            vec![NativeSessionObservation {
                reference: reference.clone(),
                title: Some("Stale listed title".to_string()),
                last_activity: Some("2026-07-21T12:30:00Z".to_string()),
            }],
        )
        .unwrap();

    assert_eq!(
        catalog.entry(&reference).unwrap().observation,
        NativeSessionObservation {
            reference,
            title: Some("Live title".to_string()),
            last_activity: Some("2026-07-21T13:00:00Z".to_string()),
        }
    );
}

#[test]
fn archived_identity_is_agent_scoped_and_survives_restart() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let catalog = NativeSessionCatalog::open(store.clone()).unwrap();
    let archived = NativeSessionRef::new("codex", "shared-session");
    let other_agent = NativeSessionRef::new("claude", "shared-session");
    catalog
        .record_page(
            "project-1",
            "/workspace/project",
            vec![
                NativeSessionObservation {
                    reference: archived.clone(),
                    title: Some("Codex session".to_string()),
                    last_activity: None,
                },
                NativeSessionObservation {
                    reference: other_agent.clone(),
                    title: Some("Claude session".to_string()),
                    last_activity: None,
                },
            ],
        )
        .unwrap();

    catalog.archive(&archived).unwrap();
    drop(catalog);

    let reopened = NativeSessionCatalog::open(store).unwrap();
    assert!(reopened.is_archived(&archived));
    assert!(!reopened.is_archived(&other_agent));
}

#[test]
fn later_listing_does_not_restore_an_archived_identity() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let catalog = NativeSessionCatalog::open(store).unwrap();
    let reference = NativeSessionRef::new("codex", "session-1");
    let observation = NativeSessionObservation {
        reference: reference.clone(),
        title: Some("Original".to_string()),
        last_activity: None,
    };
    catalog
        .record_page("project-1", "/workspace/project", vec![observation])
        .unwrap();
    catalog.archive(&reference).unwrap();

    catalog
        .record_page(
            "project-1",
            "/workspace/project",
            vec![NativeSessionObservation {
                reference: reference.clone(),
                title: Some("Updated".to_string()),
                last_activity: Some("2026-07-24T12:00:00Z".to_string()),
            }],
        )
        .unwrap();

    assert!(catalog.is_archived(&reference));
    assert_eq!(
        catalog
            .entry(&reference)
            .unwrap()
            .observation
            .title
            .as_deref(),
        Some("Updated")
    );
}

#[test]
fn failed_archive_write_leaves_in_memory_state_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_path_buf()).unwrap();
    let catalog = NativeSessionCatalog::open(store).unwrap();
    let reference = NativeSessionRef::new("codex", "session-1");
    catalog
        .record_page(
            "project-1",
            "/workspace/project",
            vec![NativeSessionObservation {
                reference: reference.clone(),
                title: None,
                last_activity: None,
            }],
        )
        .unwrap();
    let catalog_dir = temp.path().join("native-sessions");
    let backup_dir = temp.path().join("native-sessions-backup");
    std::fs::rename(&catalog_dir, &backup_dir).unwrap();
    std::fs::write(&catalog_dir, "not a directory").unwrap();

    assert!(catalog.archive(&reference).is_err());
    assert!(!catalog.is_archived(&reference));
}
