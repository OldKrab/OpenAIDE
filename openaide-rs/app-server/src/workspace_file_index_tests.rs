use std::fs;
use std::time::Duration;

use tempfile::TempDir;

use super::{WorkspaceFileIndex, WorkspaceFileIndexState};

#[test]
fn indexes_effective_git_files_and_ranks_paths() {
    let workspace = git_workspace();
    write(workspace.path(), "README.md");
    write(workspace.path(), ".env.example");
    write(workspace.path(), "src/main.rs");
    write(workspace.path(), "src/file search.rs");
    write(workspace.path(), "target/cache.bin");
    write(workspace.path(), "kept-by-dot-ignore.txt");
    fs::write(workspace.path().join(".gitignore"), "target/\n").unwrap();
    fs::write(workspace.path().join(".ignore"), "kept-by-dot-ignore.txt\n").unwrap();

    let index = WorkspaceFileIndex::new(2, Duration::from_secs(60));
    let empty = index.search(workspace.path(), "");
    assert_eq!(empty.state, WorkspaceFileIndexState::Ready);
    assert!(
        empty.paths.iter().position(|path| path == "README.md")
            < empty.paths.iter().position(|path| path == "src/main.rs")
    );
    assert!(empty.paths.contains(&".env.example".to_string()));
    assert!(empty.paths.contains(&"kept-by-dot-ignore.txt".to_string()));
    assert!(!empty.paths.iter().any(|path| path.starts_with("target/")));
    assert!(!empty.paths.iter().any(|path| path.starts_with(".git/")));

    assert_eq!(
        index.search(workspace.path(), "main").paths.first(),
        Some(&"src/main.rs".to_string())
    );
    assert!(index
        .search(workspace.path(), "file search")
        .paths
        .contains(&"src/file search.rs".to_string()));
}

#[test]
fn git_metadata_changes_do_not_refresh_workspace_results() {
    let workspace = git_workspace();
    write(workspace.path(), "README.md");
    let index = WorkspaceFileIndex::new(2, Duration::from_secs(60));
    assert_eq!(
        index.search(workspace.path(), "readme").state,
        WorkspaceFileIndexState::Ready
    );

    write(workspace.path(), ".git/HEAD");
    std::thread::sleep(Duration::from_millis(50));

    assert_eq!(
        index.search(workspace.path(), "readme").state,
        WorkspaceFileIndexState::Ready
    );
}

#[test]
fn watcher_marks_changed_workspace_for_refresh() {
    let workspace = git_workspace();
    write(workspace.path(), "README.md");
    let index = WorkspaceFileIndex::new(2, Duration::from_secs(60));
    assert!(index.search(workspace.path(), "new-file").paths.is_empty());

    write(workspace.path(), "src/new-file.rs");
    let mut saw_refresh = false;
    let mut saw_file = false;
    for _ in 0..40 {
        let search = index.search(workspace.path(), "new-file");
        saw_refresh |= search.state == WorkspaceFileIndexState::Refreshing;
        saw_file |= search.paths == ["src/new-file.rs"];
        if saw_file {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(saw_refresh || saw_file);
    assert!(saw_file);
}

#[test]
fn forget_drops_a_cached_workspace() {
    let workspace = git_workspace();
    write(workspace.path(), "old.rs");
    let canonical = workspace.path().canonicalize().unwrap();
    let index = WorkspaceFileIndex::new(2, Duration::from_secs(60));
    assert_eq!(index.search(workspace.path(), "old").paths, ["old.rs"]);

    index.forget(&canonical);
    fs::remove_file(workspace.path().join("old.rs")).unwrap();
    write(workspace.path(), "new.rs");
    assert_eq!(index.search(workspace.path(), "new").paths, ["new.rs"]);
}

fn git_workspace() -> TempDir {
    let workspace = tempfile::tempdir().unwrap();
    fs::create_dir(workspace.path().join(".git")).unwrap();
    workspace
}

fn write(root: &std::path::Path, relative: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, relative).unwrap();
}
