use super::*;

#[test]
fn empty_changes_serialize_to_an_empty_object() {
    let changes = GlobalStateChanges::default();
    assert!(changes.is_empty());
    assert_eq!(serde_json::to_string(&changes).unwrap(), "{}");
}

#[test]
fn sync_notification_round_trips() {
    let notification = SyncNotification {
        changes: GlobalStateChanges {
            removed_worktree_repositories: vec![WorktreeRepositoryId::from("repo-1")],
            ..GlobalStateChanges::default()
        },
    };
    let json = serde_json::to_value(&notification).unwrap();
    assert_eq!(
        json,
        serde_json::json!({ "changes": { "removedWorktreeRepositories": ["repo-1"] } })
    );
    assert_eq!(
        serde_json::from_value::<SyncNotification>(json).unwrap(),
        notification
    );
}
