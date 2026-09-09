use super::*;
fn params(path: &str) -> ProjectFilesParams {
    ProjectFilesParams {
        task_id: "task".into(),
        path: path.into(),
        query: String::new(),
        content: false,
        case_sensitive: false,
        include_ignored: false,
        cursor: 0,
    }
}
#[test]
fn directory_stays_in_workspace_and_pages_files() {
    let root = tempfile::tempdir().unwrap();
    for i in 0..205 {
        fs::write(root.path().join(format!("file-{i:03}.txt")), "hello").unwrap();
    }
    let first = run(root.path(), "fileViewer/listDirectory", &params(""));
    assert_eq!(first.entries.len(), 200);
    assert_eq!(first.next_cursor, Some(200));
    let mut next = params("");
    next.cursor = 200;
    assert_eq!(
        run(root.path(), "fileViewer/listDirectory", &next)
            .entries
            .len(),
        5
    );
    assert_eq!(
        run(root.path(), "fileViewer/listDirectory", &params("../"))
            .error
            .as_deref(),
        Some("outsideWorkspace")
    );
}
#[test]
fn content_search_reports_real_lines_and_case_matching() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("hello.rs"),
        "first\nHello world\nhello world\n",
    )
    .unwrap();
    let mut p = params("");
    p.content = true;
    p.query = "Hello".into();
    p.case_sensitive = true;
    let found = run(root.path(), "fileViewer/search", &p);
    assert_eq!(found.entries.len(), 1);
    assert_eq!(found.entries[0].line, Some(2));
    p.case_sensitive = false;
    assert_eq!(run(root.path(), "fileViewer/search", &p).entries.len(), 2);
}
#[cfg(unix)]
#[test]
fn directory_does_not_follow_external_symlinks() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("outside")).unwrap();
    assert!(run(root.path(), "fileViewer/listDirectory", &params(""))
        .entries
        .is_empty());
    assert_eq!(
        run(root.path(), "fileViewer/listDirectory", &params("outside"))
            .error
            .as_deref(),
        Some("outsideWorkspace")
    );
}
fn git_command(root: &Path, args: &[&str]) {
    assert!(std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap()
        .status
        .success());
}
#[test]
fn git_review_covers_modified_deleted_renamed_and_untracked_files() {
    let root = tempfile::tempdir().unwrap();
    let p = root.path();
    git_command(p, &["init", "-q"]);
    fs::write(p.join("a.txt"), "one\ntwo\nthree\n").unwrap();
    fs::write(p.join("delete.txt"), "gone\n").unwrap();
    fs::write(p.join("old.txt"), "rename\n").unwrap();
    git_command(p, &["add", "."]);
    git_command(
        p,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-qm",
            "initial",
        ],
    );
    fs::write(p.join("a.txt"), "one\nchanged\nthree\n").unwrap();
    fs::remove_file(p.join("delete.txt")).unwrap();
    git_command(p, &["mv", "old.txt", "new.txt"]);
    fs::write(p.join("untracked.txt"), "new\n").unwrap();
    let changes = run(p, "fileViewer/changes", &params(""));
    assert!(changes.error.is_none());
    assert!(changes
        .entries
        .iter()
        .any(|e| e.status.as_deref() == Some("renamed")));
    assert!(changes
        .entries
        .iter()
        .any(|e| e.status.as_deref() == Some("deleted")));
    let diff = run(p, "fileViewer/diff", &params("a.txt")).diff.unwrap();
    assert!(diff
        .hunks
        .iter()
        .flat_map(|h| &h.lines)
        .any(|l| l.kind == "remove" && l.text == "two" && l.old_line == Some(2)));
    assert!(diff
        .hunks
        .iter()
        .flat_map(|h| &h.lines)
        .any(|l| l.kind == "add" && l.text == "changed" && l.new_line == Some(2)));
    assert_eq!(
        run(p, "fileViewer/diff", &params("untracked.txt"))
            .diff
            .unwrap()
            .hunks[0]
            .lines[0]
            .text,
        "new"
    );
    assert!(run(p, "fileViewer/diff", &params("delete.txt"))
        .diff
        .unwrap()
        .hunks[0]
        .lines
        .iter()
        .any(|l| l.kind == "remove"));
}

#[test]
fn search_reports_limits_and_respects_ignore_rules() {
    let root = tempfile::tempdir().unwrap();
    git_command(root.path(), &["init", "-q"]);
    fs::write(root.path().join(".gitignore"), "ignored.txt\n").unwrap();
    fs::write(root.path().join("ignored.txt"), "needle").unwrap();
    fs::write(root.path().join("visible.txt"), "needle\n".repeat(205)).unwrap();
    let mut p = params("");
    p.content = true;
    p.query = "needle".into();
    let first = run(root.path(), "fileViewer/search", &p);
    assert_eq!(first.entries.len(), 200);
    assert_eq!(first.next_cursor, Some(200));
    assert!(first
        .entries
        .iter()
        .all(|entry| entry.path == "visible.txt"));
    p.cursor = 200;
    assert_eq!(run(root.path(), "fileViewer/search", &p).entries.len(), 5);
    p.cursor = 0;
    p.include_ignored = true;
    assert!(run(root.path(), "fileViewer/search", &p)
        .entries
        .iter()
        .any(|entry| entry.path == "ignored.txt"));
    fs::write(
        root.path().join("large.txt"),
        vec![b'a'; FILE_BYTES as usize + 1],
    )
    .unwrap();
    p.query = "not-present".into();
    assert!(run(root.path(), "fileViewer/search", &p).truncated);
}

#[test]
fn git_review_handles_unborn_binary_and_non_repository() {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(
        run(root.path(), "fileViewer/changes", &params(""))
            .error
            .as_deref(),
        Some("notRepository")
    );
    git_command(root.path(), &["init", "-q"]);
    fs::write(root.path().join("new.txt"), "first\n").unwrap();
    fs::write(root.path().join("binary.bin"), [0, 255]).unwrap();
    assert!(run(root.path(), "fileViewer/changes", &params(""))
        .base
        .is_none());
    assert_eq!(
        run(root.path(), "fileViewer/diff", &params("new.txt"))
            .diff
            .unwrap()
            .hunks[0]
            .lines[0]
            .new_line,
        Some(1)
    );
    assert!(
        run(root.path(), "fileViewer/diff", &params("binary.bin"))
            .diff
            .unwrap()
            .binary
    );
}

#[cfg(unix)]
#[test]
fn git_diff_treats_selected_filename_as_literal_data() {
    let root = tempfile::tempdir().unwrap();
    let p = root.path();
    git_command(p, &["init", "-q"]);
    fs::write(p.join("*.txt"), "old\n").unwrap();
    fs::write(p.join("other.txt"), "private\n").unwrap();
    git_command(p, &["add", "."]);
    git_command(
        p,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-qm",
            "initial",
        ],
    );
    fs::write(p.join("*.txt"), "selected\n").unwrap();
    fs::write(p.join("other.txt"), "unrelated\n").unwrap();
    let diff = run(p, "fileViewer/diff", &params("*.txt")).diff.unwrap();
    let lines: Vec<_> = diff.hunks.iter().flat_map(|hunk| &hunk.lines).collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[1].text, "selected");
}

#[test]
fn git_review_uses_linked_worktree_and_reports_conflicts() {
    let repository = tempfile::tempdir().unwrap();
    let p = repository.path();
    git_command(p, &["init", "-q"]);
    fs::write(p.join("same.txt"), "base\n").unwrap();
    git_command(p, &["add", "."]);
    git_command(
        p,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-qm",
            "base",
        ],
    );
    let worktree_parent = tempfile::tempdir().unwrap();
    let worktree = worktree_parent.path().join("linked");
    git_command(
        p,
        &[
            "worktree",
            "add",
            "-qb",
            "review",
            worktree.to_str().unwrap(),
        ],
    );
    fs::write(worktree.join("same.txt"), "linked\n").unwrap();
    git_command(
        &worktree,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-qam",
            "linked",
        ],
    );
    fs::write(p.join("same.txt"), "main\n").unwrap();
    git_command(
        p,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-qam",
            "main",
        ],
    );
    assert!(run(p, "fileViewer/changes", &params("")).entries.is_empty());
    fs::write(worktree.join("only-linked.txt"), "linked workspace\n").unwrap();
    let changes = run(&worktree, "fileViewer/changes", &params(""));
    assert_eq!(changes.branch.as_deref(), Some("review"));
    assert_eq!(changes.entries[0].path, "only-linked.txt");
    let main_head = std::process::Command::new("git")
        .arg("-C")
        .arg(p)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let head = String::from_utf8(main_head.stdout).unwrap();
    let merge = std::process::Command::new("git")
        .arg("-C")
        .arg(&worktree)
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "merge",
            "--no-edit",
            head.trim(),
        ])
        .output()
        .unwrap();
    assert!(!merge.status.success());
    assert!(run(&worktree, "fileViewer/changes", &params(""))
        .entries
        .iter()
        .any(|entry| entry.status.as_deref() == Some("conflicted")));
    let diff = run(&worktree, "fileViewer/diff", &params("same.txt"))
        .diff
        .unwrap();
    assert!(diff.conflicted);
    assert!(diff.hunks.is_empty());
}
