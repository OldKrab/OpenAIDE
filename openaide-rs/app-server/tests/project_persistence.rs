use openaide_app_server::projects::ConfiguredProjectRoots;
use openaide_app_server::storage::Store;

#[test]
fn managed_projects_survive_repeated_catalog_writes() {
    let temp = tempfile::tempdir().unwrap();
    let state_root = temp.path().join("state");
    let first_root = temp.path().join("first-project");
    let second_root = temp.path().join("second-project");
    std::fs::create_dir_all(&first_root).unwrap();
    std::fs::create_dir_all(&second_root).unwrap();

    let projects = ConfiguredProjectRoots::default();
    projects
        .enable_persistence(Store::open(state_root.clone()).unwrap())
        .unwrap();
    projects.add_project(&first_root.to_string_lossy()).unwrap();
    projects
        .add_project(&second_root.to_string_lossy())
        .unwrap();
    drop(projects);

    let reloaded = ConfiguredProjectRoots::default();
    reloaded
        .enable_persistence(Store::open(state_root).unwrap())
        .unwrap();
    let persisted_roots = reloaded
        .projects()
        .into_iter()
        .map(|project| project.workspace_root)
        .collect::<Vec<_>>();

    assert_eq!(
        persisted_roots,
        vec![
            first_root.to_string_lossy().to_string(),
            second_root.to_string_lossy().to_string(),
        ]
    );
}
