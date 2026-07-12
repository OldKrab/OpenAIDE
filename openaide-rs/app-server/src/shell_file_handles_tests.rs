use super::*;

#[test]
fn client_reveal_handle_is_unpredictable_owned_and_single_use() {
    let registry = ShellFileRevealRegistry::new();
    let owner = ClientInstanceId::from("owner-client");

    let handle = registry
        .register_local_file_for_client(
            owner.clone(),
            PathBuf::from("/workspace/app/src/main.rs"),
            None,
        )
        .unwrap();

    assert_eq!(handle.label, "main.rs");
    assert!(!handle.id.contains("workspace"));
    let token = handle.id.strip_prefix("file-reveal-").unwrap();
    assert!(uuid::Uuid::parse_str(token).is_ok());
    assert!(registry
        .consume_for_client(&ClientInstanceId::from("other-client"), &handle.id)
        .is_none());
    let target = registry.consume_for_client(&owner, &handle.id).unwrap();
    assert_eq!(target.path, PathBuf::from("/workspace/app/src/main.rs"));
    assert_eq!(target.label, "main.rs");
    assert!(registry.consume_for_client(&owner, &handle.id).is_none());
}

#[test]
fn register_local_file_rejects_relative_paths() {
    let registry = ShellFileRevealRegistry::new();

    let error = registry
        .register_local_file_for_client(
            ClientInstanceId::from("owner-client"),
            PathBuf::from("src/main.rs"),
            None,
        )
        .unwrap_err();

    assert!(error.to_string().contains("file path"));
}

#[test]
fn registry_evicts_the_oldest_unresolved_handle_at_its_bound() {
    let registry = ShellFileRevealRegistry::new();
    let owner = ClientInstanceId::from("owner-client");
    let first = registry
        .register_local_file_for_client(
            owner.clone(),
            PathBuf::from("/workspace/app/first.rs"),
            None,
        )
        .unwrap();

    for index in 0..256 {
        registry
            .register_local_file_for_client(
                owner.clone(),
                PathBuf::from(format!("/workspace/app/file-{index}.rs")),
                None,
            )
            .unwrap();
    }

    assert!(registry.consume_for_client(&owner, &first.id).is_none());
}
