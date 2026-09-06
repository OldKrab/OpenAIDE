use super::*;

const LEGACY_FILES: &[(&str, &[u8])] = &[
    (
        "task.json",
        include_bytes!("fixtures/file-task-v0/task.json"),
    ),
    (
        "messages.jsonl",
        include_bytes!("fixtures/file-task-v0/messages.jsonl"),
    ),
    (
        "message_journal.jsonl",
        include_bytes!("fixtures/file-task-v0/message_journal.jsonl"),
    ),
    (
        "message_meta.json",
        include_bytes!("fixtures/file-task-v0/message_meta.json"),
    ),
    (
        "tool-artifacts/tool-1_0.json",
        include_bytes!("fixtures/file-task-v0/tool-artifacts/tool-1_0.json"),
    ),
];

fn install_legacy_fixture(root: &Path) -> PathBuf {
    let task_dir = root.join("tasks/task-legacy");
    for (name, bytes) in LEGACY_FILES {
        let path = task_dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    task_dir
}

#[test]
fn previous_file_task_migrates_chat_artifacts_archive_and_binding_without_losing_source_bytes() {
    let root = tempfile::tempdir().unwrap();
    install_legacy_fixture(root.path());
    for _ in 0..2 {
        let store = Store::open(root.path().to_path_buf()).unwrap();
        let task = store
            .read_task("task-legacy")
            .expect("previous Task remains usable");
        assert_eq!(task.lifecycle, records::TaskLifecycle::Archived);
        assert_eq!(task.title.as_ref().unwrap().value(), "Keep this history");
        assert_eq!(
            task.agent_session_id.as_deref(),
            Some("native-session-legacy")
        );
        let messages = store.read_messages("task-legacy").unwrap();
        assert_eq!(messages.len(), 4);
        assert!(
            matches!(&messages[1].chat.message, NormalizedMessage::AgentMessage { parts, .. }
            if parts == &[AgentMessagePart::Text { text: "Hello world".to_string() }])
        );
        assert_eq!(
            store.local_history_updated_at("task-legacy").unwrap(),
            "1750000000010"
        );
        let details = store.read_tool_artifact("task-legacy", "tool-1_0").unwrap();
        assert!(
            matches!(details.content.as_slice(), [ActivityToolContent::Text { text }] if text == "Original tool output")
        );
        assert_eq!(
            details.output.unwrap().stdout.as_deref(),
            Some("Original stdout")
        );
    }
    let backup = root
        .path()
        .join("task-store-v1/tasks/task-legacy/legacy-files");
    for (name, bytes) in LEGACY_FILES {
        assert_eq!(std::fs::read(backup.join(name)).unwrap(), *bytes);
    }
}

#[test]
fn legacy_import_recovers_chat_committed_before_its_derived_metadata() {
    let root = tempfile::tempdir().unwrap();
    let source = install_legacy_fixture(root.path());
    let stale_metadata = include_bytes!("fixtures/file-task-v0/message_meta.stale.json");
    // The old writer synced the fourth message's journal record before replacing
    // message_meta.json. Process death here left usable history with stale counts.
    std::fs::write(source.join("message_meta.json"), stale_metadata).unwrap();
    let backup = root
        .path()
        .join("task-store-v1/tasks/task-legacy/legacy-files");

    for attempt in 0..2 {
        let store = Store::open(root.path().to_path_buf()).unwrap();
        let projection = store.task_journal().load("task-legacy").unwrap();
        assert_eq!(projection.messages.len(), 4);
        assert_eq!(projection.message_meta.message_count, 4);
        assert_eq!(projection.message_meta.first_cursor.as_deref(), Some("1"));
        assert_eq!(projection.message_meta.last_cursor.as_deref(), Some("4"));
        // A crash does not let us reconstruct an uncommitted clock advance.
        assert_eq!(store.message_history_version("task-legacy").unwrap(), 7);
        assert_eq!(
            store.local_history_updated_at("task-legacy").unwrap(),
            "1750000000010"
        );
        assert!(matches!(
            &projection.messages[3].chat.message,
            NormalizedMessage::AgentMessage { parts, .. }
                if parts == &[AgentMessagePart::Text { text: "Finished".to_string() }]
        ));
        assert_eq!(
            std::fs::read(backup.join("message_meta.json")).unwrap(),
            stale_metadata
        );
        drop(store);
        if attempt == 0 {
            // Recovery is deterministic even if migration itself then crashes
            // after the destination commit and before the source directory move.
            std::fs::rename(&backup, &source).unwrap();
        }
    }
}

#[test]
fn stale_legacy_metadata_does_not_hide_identity_mismatch_or_journal_corruption() {
    for (filename, original, replacement, expected_error) in [
        (
            "message_meta.json",
            "\"task_id\":\"task-legacy\"",
            "\"task_id\":\"different-task\"",
            "Chat metadata identity mismatch",
        ),
        (
            "message_journal.jsonl",
            "\"sequence\":3",
            "\"sequence\":9",
            "Chat journal sequence gap",
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        let source = install_legacy_fixture(root.path());
        std::fs::write(
            source.join("message_meta.json"),
            include_bytes!("fixtures/file-task-v0/message_meta.stale.json"),
        )
        .unwrap();
        let path = source.join(filename);
        let corrupted = std::fs::read_to_string(&path)
            .unwrap()
            .replace(original, replacement);
        std::fs::write(&path, &corrupted).unwrap();

        let error = Store::open(root.path().to_path_buf())
            .err()
            .expect("only reconstructible metadata can be repaired");
        assert!(error.to_string().contains(expected_error));
        assert!(error.to_string().contains("original files were preserved"));
        assert_eq!(std::fs::read_to_string(path).unwrap(), corrupted);
        assert!(source.is_dir());
    }
}

#[test]
fn interrupted_legacy_import_finishes_once_and_reset_cannot_resurrect_its_source() {
    let root = tempfile::tempdir().unwrap();
    let source = install_legacy_fixture(root.path());
    let store = Store::open(root.path().to_path_buf()).unwrap();
    drop(store);
    let backup = root
        .path()
        .join("task-store-v1/tasks/task-legacy/legacy-files");
    // Model process death after current storage committed, before moving the
    // originals out of the source directory. No migration marker is required.
    std::fs::rename(&backup, &source).unwrap();
    let reopened = Store::open(root.path().to_path_buf()).unwrap();
    assert_eq!(reopened.list_archived_tasks().unwrap().len(), 1);
    assert_eq!(reopened.read_messages("task-legacy").unwrap().len(), 4);
    assert!(backup.is_dir());
    reopened.reset_task_history().unwrap();
    drop(reopened);
    let reopened = Store::open(root.path().to_path_buf()).unwrap();
    assert!(reopened.list_all_task_records().unwrap().is_empty());
    assert!(!backup.exists());
}

#[test]
fn legacy_import_never_overwrites_a_conflicting_current_task() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path().to_path_buf()).unwrap();
    let existing = task_record("task-legacy", TaskStatus::Inactive, "99");
    store.write_task(&existing).unwrap();
    drop(store);
    let source = install_legacy_fixture(root.path());
    let error = Store::open(root.path().to_path_buf())
        .err()
        .expect("identity conflict must be visible");
    assert!(error.to_string().contains("conflicts with legacy history"));
    for (name, bytes) in LEGACY_FILES {
        assert_eq!(std::fs::read(source.join(name)).unwrap(), *bytes);
    }
    let (journal, _) = task_journal::TaskJournalStore::open(root.path().to_path_buf()).unwrap();
    assert_eq!(journal.load("task-legacy").unwrap().task.updated_at, "99");
    assert!(journal.load("task-legacy").unwrap().messages.is_empty());
    journal.shutdown().unwrap();
}
