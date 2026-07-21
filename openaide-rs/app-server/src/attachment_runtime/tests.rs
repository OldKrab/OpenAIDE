use std::time::Duration;

use openaide_app_server_protocol::attachment::{
    AttachmentReleaseOutcome, AttachmentReleaseStatus, AttachmentResourceId,
};
use openaide_app_server_protocol::ids::{ClientInstanceId, TaskId};

use super::{AttachmentOwner, AttachmentRuntime, AttachmentRuntimeError};

#[test]
fn abandoned_attachment_resource_ttl_is_thirty_minutes() {
    assert_eq!(AttachmentRuntime::new().ttl, Duration::from_secs(30 * 60));
}

#[test]
fn lists_task_root_and_directory_entries_with_opaque_ids() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("b.txt"), "hello").unwrap();
    std::fs::create_dir(temp.path().join("a-dir")).unwrap();
    let runtime = AttachmentRuntime::new();
    let task_id = TaskId::from("task-1");

    let roots = runtime.list_roots(&task_id, temp.path());
    assert_eq!(roots.roots.len(), 1);
    assert_eq!(roots.roots[0].root_id.as_str(), "task-root-1");
    assert!(!roots.roots[0].label.contains('/'));

    let listing = runtime
        .list_directory(&task_id, temp.path(), &roots.roots[0].root_id, None)
        .unwrap();

    assert_eq!(listing.entries[0].label, "a-dir");
    assert_eq!(listing.entries[0].kind.sort_key(), 0);
    assert_eq!(listing.entries[1].label, "b.txt");
    assert!(listing.entries[1].selectable);
    assert!(listing.entries[1]
        .entry_id
        .as_str()
        .starts_with("file-entry-"));
}

#[cfg(unix)]
#[test]
fn file_browser_omits_symlinks_resolving_outside_the_allowed_root() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    let outside = temp.path().join("outside");
    std::fs::create_dir(&workspace).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(workspace.join("inside.txt"), "inside").unwrap();
    std::fs::write(outside.join("secret.txt"), "outside").unwrap();
    symlink(
        outside.join("secret.txt"),
        workspace.join("escaped-file.txt"),
    )
    .unwrap();
    symlink(&outside, workspace.join("escaped-directory")).unwrap();

    let runtime = AttachmentRuntime::new();
    let task_id = TaskId::from("task-1");
    let root = runtime.list_roots(&task_id, &workspace).roots.remove(0);
    let listing = runtime
        .list_directory(&task_id, &workspace, &root.root_id, None)
        .unwrap();

    let labels = listing
        .entries
        .iter()
        .map(|entry| entry.label.as_str())
        .collect::<Vec<_>>();
    assert_eq!(labels, vec!["inside.txt"]);
}

#[cfg(unix)]
#[test]
fn file_browser_lists_symlinks_resolving_inside_the_allowed_root() {
    use std::os::unix::fs::symlink;

    let workspace = tempfile::tempdir().unwrap();
    std::fs::create_dir(workspace.path().join("real-directory")).unwrap();
    std::fs::write(workspace.path().join("real-file.txt"), "inside").unwrap();
    symlink(
        workspace.path().join("real-directory"),
        workspace.path().join("linked-directory"),
    )
    .unwrap();
    symlink(
        workspace.path().join("real-file.txt"),
        workspace.path().join("linked-file.txt"),
    )
    .unwrap();

    let runtime = AttachmentRuntime::new();
    let task_id = TaskId::from("task-1");
    let root = runtime
        .list_roots(&task_id, workspace.path())
        .roots
        .remove(0);
    let listing = runtime
        .list_directory(&task_id, workspace.path(), &root.root_id, None)
        .unwrap();

    assert!(listing
        .entries
        .iter()
        .any(|entry| entry.label == "linked-directory" && !entry.selectable));
    assert!(listing
        .entries
        .iter()
        .any(|entry| entry.label == "linked-file.txt" && entry.selectable));
}

#[cfg(unix)]
#[test]
fn file_browser_rejects_a_directory_replaced_with_an_escaping_symlink() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    let outside = temp.path().join("outside");
    let selected = workspace.join("selected-directory");
    std::fs::create_dir(&workspace).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::create_dir(&selected).unwrap();
    std::fs::write(outside.join("secret.txt"), "outside").unwrap();

    let runtime = AttachmentRuntime::new();
    let task_id = TaskId::from("task-1");
    let root = runtime.list_roots(&task_id, &workspace).roots.remove(0);
    let listing = runtime
        .list_directory(&task_id, &workspace, &root.root_id, None)
        .unwrap();
    let directory_id = listing.entries[0].entry_id.clone();

    std::fs::remove_dir(&selected).unwrap();
    symlink(&outside, &selected).unwrap();

    assert_eq!(
        runtime
            .list_directory(&task_id, &workspace, &root.root_id, Some(&directory_id),)
            .unwrap_err(),
        AttachmentRuntimeError::OutsideAllowedRoot
    );
}

#[test]
fn creates_file_reference_handle_from_file_entry() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("notes.md"), "hello").unwrap();
    let runtime = AttachmentRuntime::new();
    let task_id = TaskId::from("task-1");
    let root = runtime.list_roots(&task_id, temp.path()).roots.remove(0);
    let listing = runtime
        .list_directory(&task_id, temp.path(), &root.root_id, None)
        .unwrap();

    let created = runtime
        .create_file_reference(&task_id, &listing.entries[0].entry_id)
        .unwrap();
    let resolved = runtime
        .resolve_for_send(&task_id, &[created.attachment.handle_id])
        .unwrap();

    assert_eq!(created.attachment.label, "notes.md");
    assert_eq!(
        resolved.chat_attachments()[0].path.as_deref(),
        Some(temp.path().join("notes.md").to_string_lossy().as_ref())
    );
    assert_eq!(
        resolved.agent_attachments()[0].path.as_deref(),
        Some(temp.path().join("notes.md").to_string_lossy().as_ref())
    );
}

#[cfg(unix)]
#[test]
fn file_reference_creation_rejects_an_entry_replaced_with_an_escaping_symlink() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    let outside = temp.path().join("outside");
    std::fs::create_dir(&workspace).unwrap();
    std::fs::create_dir(&outside).unwrap();
    let selected = workspace.join("notes.txt");
    let secret = outside.join("secret.txt");
    std::fs::write(&selected, "inside").unwrap();
    std::fs::write(&secret, "outside").unwrap();

    let runtime = AttachmentRuntime::new();
    let task_id = TaskId::from("task-1");
    let root = runtime.list_roots(&task_id, &workspace).roots.remove(0);
    let listing = runtime
        .list_directory(&task_id, &workspace, &root.root_id, None)
        .unwrap();
    std::fs::remove_file(&selected).unwrap();
    symlink(&secret, &selected).unwrap();

    assert_eq!(
        runtime
            .create_file_reference(&task_id, &listing.entries[0].entry_id)
            .unwrap_err(),
        AttachmentRuntimeError::OutsideAllowedRoot
    );
}

#[cfg(unix)]
#[test]
fn file_reference_reveal_and_send_reject_a_target_replaced_with_an_escaping_symlink() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    let outside = temp.path().join("outside");
    std::fs::create_dir(&workspace).unwrap();
    std::fs::create_dir(&outside).unwrap();
    let selected = workspace.join("notes.txt");
    let secret = outside.join("secret.txt");
    std::fs::write(&selected, "inside").unwrap();
    std::fs::write(&secret, "outside").unwrap();

    let runtime = AttachmentRuntime::new();
    let task_id = TaskId::from("task-1");
    let root = runtime.list_roots(&task_id, &workspace).roots.remove(0);
    let listing = runtime
        .list_directory(&task_id, &workspace, &root.root_id, None)
        .unwrap();
    let handle = runtime
        .create_file_reference(&task_id, &listing.entries[0].entry_id)
        .unwrap()
        .attachment
        .handle_id;

    std::fs::remove_file(&selected).unwrap();
    symlink(&secret, &selected).unwrap();

    assert_eq!(
        runtime.resolve_for_reveal(&task_id, &handle).unwrap_err(),
        AttachmentRuntimeError::OutsideAllowedRoot
    );
    assert_eq!(
        runtime.resolve_for_send(&task_id, &[handle]).unwrap_err(),
        AttachmentRuntimeError::OutsideAllowedRoot
    );
}

#[test]
fn creates_pasted_image_handle_for_prompt_payload() {
    let runtime = AttachmentRuntime::new();
    let created = runtime
        .create_pasted_image(
            TaskId::from("task-1"),
            "/tmp/Screenshot.png",
            "image/png",
            "aW1hZ2U=",
        )
        .unwrap();

    let resolved = runtime
        .resolve_for_send(TaskId::from("task-1"), &[created.attachment.handle_id])
        .unwrap();

    assert_eq!(created.attachment.label, "Screenshot.png");
    let chat_attachments = resolved.chat_attachments();
    assert_eq!(chat_attachments[0].kind, "image");
    let chat_payload = chat_attachments[0].payload.as_ref().unwrap();
    assert_eq!(chat_payload["mimeType"], "image/png");
    assert_eq!(chat_payload["data"], "aW1hZ2U=");
    assert_eq!(chat_payload["sizeBytes"], 5);
    let agent_attachments = resolved.agent_attachments();
    let payload = agent_attachments[0].payload.as_ref().unwrap();
    assert_eq!(agent_attachments[0].kind, "image");
    assert_eq!(payload["mimeType"], "image/png");
    assert_eq!(payload["data"], "aW1hZ2U=");
    assert_eq!(payload["sizeBytes"], 5);
}

#[test]
fn creates_image_handle_from_completed_binary_upload() {
    let temp = tempfile::tempdir().unwrap();
    let uploaded = temp.path().join("uploaded-image.png");
    std::fs::write(&uploaded, b"image").unwrap();
    let runtime = AttachmentRuntime::new();

    let created = runtime
        .create_uploaded_image(
            TaskId::from("task-1"),
            &uploaded,
            "Screenshot.png",
            "image/png",
        )
        .unwrap();
    let resolved = runtime
        .resolve_for_send(TaskId::from("task-1"), &[created.attachment.handle_id])
        .unwrap();

    let agent_attachments = resolved.agent_attachments();
    let payload = agent_attachments[0].payload.as_ref().unwrap();
    assert_eq!(agent_attachments[0].kind, "image");
    assert_eq!(payload["mimeType"], "image/png");
    assert_eq!(payload["data"], "aW1hZ2U=");
    assert_eq!(payload["sizeBytes"], 5);
}

#[test]
fn rejects_invalid_pasted_image_payloads() {
    let runtime = AttachmentRuntime::new();

    let wrong_mime = runtime
        .create_pasted_image(TaskId::from("task-1"), "x.png", "text/plain", "aW1hZ2U=")
        .unwrap_err();
    assert_eq!(wrong_mime, AttachmentRuntimeError::InvalidImage);

    let invalid_data = runtime
        .create_pasted_image(TaskId::from("task-1"), "x.png", "image/png", "not base64")
        .unwrap_err();
    assert_eq!(invalid_data, AttachmentRuntimeError::InvalidImage);
}

#[test]
fn resolves_file_reference_handle_for_reveal() {
    let runtime = AttachmentRuntime::new();
    let (files, handle) = registered_file_reference(&runtime, "task-1");

    let target = runtime
        .resolve_for_reveal(TaskId::from("task-1"), &handle.handle_id)
        .unwrap();

    assert_eq!(target.label, "notes.md");
    assert_eq!(target.path, files.path().join("notes.md"));
}

#[test]
fn confirms_embedded_candidate_into_sendable_text_handle() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("notes.txt"), "hello embedded").unwrap();
    let runtime = AttachmentRuntime::new();
    let task_id = TaskId::from("task-1");
    let root = runtime.list_roots(&task_id, temp.path()).roots.remove(0);
    let listing = runtime
        .list_directory(&task_id, temp.path(), &root.root_id, None)
        .unwrap();

    let candidate = runtime
        .create_embedded_candidate(&task_id, &listing.entries[0].entry_id)
        .unwrap()
        .candidate;
    let confirmed =
        runtime.confirm_embedded(&task_id, std::slice::from_ref(&candidate.candidate_id));
    assert!(confirmed.errors.is_empty());

    let resolved = runtime
        .resolve_for_send(&task_id, &[confirmed.attachments[0].handle_id.clone()])
        .unwrap();
    assert_eq!(confirmed.attachments[0].label, "notes.txt");
    assert_eq!(
        resolved.chat_attachments()[0].path.as_deref(),
        Some(temp.path().join("notes.txt").to_string_lossy().as_ref())
    );
    assert_eq!(
        resolved.agent_attachments()[0].payload.as_ref().unwrap()["text"],
        "hello embedded"
    );
}

#[test]
fn concurrent_embedded_candidate_confirmations_create_one_handle() {
    let files = tempfile::tempdir().unwrap();
    std::fs::write(files.path().join("notes.txt"), "hello embedded").unwrap();
    let runtime = AttachmentRuntime::new();
    let task_id = TaskId::from("task-1");
    let root = runtime.list_roots(&task_id, files.path()).roots.remove(0);
    let listing = runtime
        .list_directory(&task_id, files.path(), &root.root_id, None)
        .unwrap();
    let candidate_id = runtime
        .create_embedded_candidate(&task_id, &listing.entries[0].entry_id)
        .unwrap()
        .candidate
        .candidate_id;
    let gate = runtime.pause_embedded_confirmations_for_test(2);

    let confirmations = (0..2)
        .map(|_| {
            let runtime = runtime.clone();
            let task_id = task_id.clone();
            let candidate_id = candidate_id.clone();
            std::thread::spawn(move || runtime.confirm_embedded(task_id, &[candidate_id]))
        })
        .collect::<Vec<_>>();
    gate.wait_until_arrived();
    gate.release();
    let results = confirmations
        .into_iter()
        .map(|confirmation| confirmation.join().unwrap())
        .collect::<Vec<_>>();

    let attachments = results
        .iter()
        .flat_map(|result| result.attachments.iter())
        .collect::<Vec<_>>();
    assert_eq!(attachments.len(), 1);
    assert_eq!(
        results
            .iter()
            .map(|result| result.errors.len())
            .sum::<usize>(),
        1
    );
    assert!(runtime
        .resolve_for_send(&task_id, std::slice::from_ref(&attachments[0].handle_id))
        .is_ok());
}

#[test]
fn release_during_embedded_confirmation_prevents_handle_creation() {
    let files = tempfile::tempdir().unwrap();
    std::fs::write(files.path().join("notes.txt"), "hello embedded").unwrap();
    let runtime = AttachmentRuntime::new();
    let task_id = TaskId::from("task-1");
    let root = runtime.list_roots(&task_id, files.path()).roots.remove(0);
    let listing = runtime
        .list_directory(&task_id, files.path(), &root.root_id, None)
        .unwrap();
    let candidate_id = runtime
        .create_embedded_candidate(&task_id, &listing.entries[0].entry_id)
        .unwrap()
        .candidate
        .candidate_id;
    let gate = runtime.pause_embedded_confirmations_for_test(1);
    let confirming_runtime = runtime.clone();
    let confirming_task_id = task_id.clone();
    let confirming_candidate_id = candidate_id.clone();
    let confirmation = std::thread::spawn(move || {
        confirming_runtime.confirm_embedded(confirming_task_id, &[confirming_candidate_id])
    });
    gate.wait_until_arrived();

    let resource = candidate_resource(candidate_id);
    let released = runtime.release_resources(&task_id, std::slice::from_ref(&resource));
    assert_eq!(released.outcomes, vec![released_outcome(resource)]);
    gate.release();
    let result = confirmation.join().unwrap();

    assert!(result.attachments.is_empty());
    assert_eq!(result.errors.len(), 1);
}

#[cfg(unix)]
#[test]
fn embedded_send_rejects_a_source_replaced_with_an_escaping_symlink() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    let outside = temp.path().join("outside");
    std::fs::create_dir(&workspace).unwrap();
    std::fs::create_dir(&outside).unwrap();
    let selected = workspace.join("notes.txt");
    let secret = outside.join("secret.txt");
    std::fs::write(&selected, "inside").unwrap();
    std::fs::write(&secret, "outside").unwrap();

    let runtime = AttachmentRuntime::new();
    let task_id = TaskId::from("task-1");
    let root = runtime.list_roots(&task_id, &workspace).roots.remove(0);
    let listing = runtime
        .list_directory(&task_id, &workspace, &root.root_id, None)
        .unwrap();
    let candidate = runtime
        .create_embedded_candidate(&task_id, &listing.entries[0].entry_id)
        .unwrap()
        .candidate;
    let confirmed =
        runtime.confirm_embedded(&task_id, std::slice::from_ref(&candidate.candidate_id));
    let handle = confirmed.attachments[0].handle_id.clone();

    std::fs::remove_file(&selected).unwrap();
    symlink(&secret, &selected).unwrap();

    assert_eq!(
        runtime.resolve_for_send(&task_id, &[handle]).unwrap_err(),
        AttachmentRuntimeError::OutsideAllowedRoot
    );
}

#[test]
fn embedded_handles_are_not_revealable() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("notes.txt"), "hello embedded").unwrap();
    let runtime = AttachmentRuntime::new();
    let task_id = TaskId::from("task-1");
    let root = runtime.list_roots(&task_id, temp.path()).roots.remove(0);
    let listing = runtime
        .list_directory(&task_id, temp.path(), &root.root_id, None)
        .unwrap();
    let candidate = runtime
        .create_embedded_candidate(&task_id, &listing.entries[0].entry_id)
        .unwrap()
        .candidate;
    let confirmed =
        runtime.confirm_embedded(&task_id, std::slice::from_ref(&candidate.candidate_id));

    assert_eq!(
        runtime
            .resolve_for_reveal(&task_id, &confirmed.attachments[0].handle_id)
            .unwrap_err(),
        AttachmentRuntimeError::NotFile
    );
}

#[test]
fn rejects_non_utf8_embedded_candidate() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("blob.bin"), [0xff, 0xfe, 0xfd]).unwrap();
    let runtime = AttachmentRuntime::new();
    let task_id = TaskId::from("task-1");
    let root = runtime.list_roots(&task_id, temp.path()).roots.remove(0);
    let listing = runtime
        .list_directory(&task_id, temp.path(), &root.root_id, None)
        .unwrap();

    let error = runtime
        .create_embedded_candidate(&task_id, &listing.entries[0].entry_id)
        .unwrap_err();

    assert_eq!(error, AttachmentRuntimeError::NotText);
}

#[test]
fn wrong_task_confirm_does_not_consume_embedded_candidate() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("notes.txt"), "hello").unwrap();
    let runtime = AttachmentRuntime::new();
    let task_id = TaskId::from("task-1");
    let root = runtime.list_roots(&task_id, temp.path()).roots.remove(0);
    let listing = runtime
        .list_directory(&task_id, temp.path(), &root.root_id, None)
        .unwrap();
    let candidate = runtime
        .create_embedded_candidate(&task_id, &listing.entries[0].entry_id)
        .unwrap()
        .candidate;

    let wrong_task = runtime.confirm_embedded(
        TaskId::from("task-2"),
        std::slice::from_ref(&candidate.candidate_id),
    );
    assert_eq!(wrong_task.errors.len(), 1);

    let confirmed =
        runtime.confirm_embedded(&task_id, std::slice::from_ref(&candidate.candidate_id));
    assert!(confirmed.errors.is_empty());
    assert_eq!(confirmed.attachments[0].label, "notes.txt");
}

#[test]
fn refresh_and_release_presend_handles_are_task_scoped() {
    let runtime = AttachmentRuntime::new();
    let (_files, handle) = registered_file_reference(&runtime, "task-1");

    let refreshed = runtime
        .refresh_handles(
            TaskId::from("task-1"),
            std::slice::from_ref(&handle.handle_id),
        )
        .unwrap();
    assert_eq!(refreshed.attachments[0].label, "notes.md");
    assert_eq!(
        runtime
            .refresh_handles(
                TaskId::from("task-2"),
                std::slice::from_ref(&handle.handle_id)
            )
            .unwrap_err(),
        AttachmentRuntimeError::WrongTask
    );

    let resource = handle_resource(handle.handle_id.clone());
    let denied = runtime.release_resources(TaskId::from("task-2"), std::slice::from_ref(&resource));
    assert_eq!(denied.outcomes, vec![forbidden_outcome(resource.clone())]);
    let released =
        runtime.release_resources(TaskId::from("task-1"), std::slice::from_ref(&resource));
    assert_eq!(released.outcomes, vec![released_outcome(resource)]);
    assert_eq!(
        runtime
            .refresh_handles(
                TaskId::from("task-1"),
                std::slice::from_ref(&handle.handle_id)
            )
            .unwrap_err(),
        AttachmentRuntimeError::UnknownHandle
    );
}

#[test]
fn presend_handle_access_and_release_are_client_scoped_within_a_task() {
    let runtime = AttachmentRuntime::new();
    let files = tempfile::tempdir().unwrap();
    let path = files.path().join("notes.md");
    std::fs::write(&path, "hello").unwrap();
    let task_id = TaskId::from("task-1");
    let owner = AttachmentOwner::new(&ClientInstanceId::from("client-1"), &task_id);
    let other_client = AttachmentOwner::new(&ClientInstanceId::from("client-2"), &task_id);
    let handle = runtime.register_file_reference_for_test(&owner, "notes.md", path);

    assert_eq!(
        runtime
            .refresh_handles(&other_client, std::slice::from_ref(&handle.handle_id))
            .unwrap_err(),
        AttachmentRuntimeError::UnknownHandle
    );
    let resource = handle_resource(handle.handle_id.clone());
    assert_eq!(
        runtime
            .release_resources(&other_client, std::slice::from_ref(&resource))
            .outcomes,
        vec![forbidden_outcome(resource)]
    );
    assert_eq!(
        runtime
            .refresh_handles(&owner, std::slice::from_ref(&handle.handle_id))
            .unwrap()
            .attachments[0]
            .handle_id,
        handle.handle_id
    );
}

#[test]
fn embedded_candidate_release_is_client_scoped_and_idempotent() {
    let files = tempfile::tempdir().unwrap();
    std::fs::write(files.path().join("notes.txt"), "hello").unwrap();
    let runtime = AttachmentRuntime::new();
    let task_id = TaskId::from("task-1");
    let owner = AttachmentOwner::new(&ClientInstanceId::from("client-1"), &task_id);
    let other_client = AttachmentOwner::new(&ClientInstanceId::from("client-2"), &task_id);
    let root = runtime.list_roots(&task_id, files.path()).roots.remove(0);
    let listing = runtime
        .list_directory(&owner, files.path(), &root.root_id, None)
        .unwrap();
    let candidate = runtime
        .create_embedded_candidate(&owner, &listing.entries[0].entry_id)
        .unwrap()
        .candidate;

    let resource = candidate_resource(candidate.candidate_id);
    let denied = runtime.release_resources(&other_client, std::slice::from_ref(&resource));
    assert_eq!(denied.outcomes, vec![forbidden_outcome(resource.clone())]);

    let released = runtime.release_resources(&owner, std::slice::from_ref(&resource));
    assert_eq!(released.outcomes, vec![released_outcome(resource.clone())]);

    let repeated = runtime.release_resources(&owner, std::slice::from_ref(&resource));
    assert_eq!(repeated.outcomes, vec![no_op_outcome(resource)]);
}

#[test]
fn attachment_release_returns_ordered_outcomes_without_short_circuiting() {
    let files = tempfile::tempdir().unwrap();
    let owned_path = files.path().join("owned.txt");
    let foreign_path = files.path().join("foreign.txt");
    std::fs::write(&owned_path, "owned").unwrap();
    std::fs::write(&foreign_path, "foreign").unwrap();
    let runtime = AttachmentRuntime::new();
    let task_id = TaskId::from("task-1");
    let owner = AttachmentOwner::new(&ClientInstanceId::from("client-1"), &task_id);
    let other_client = AttachmentOwner::new(&ClientInstanceId::from("client-2"), &task_id);
    let owned = runtime.register_file_reference_for_test(&owner, "owned.txt", owned_path);
    let foreign =
        runtime.register_file_reference_for_test(&other_client, "foreign.txt", foreign_path);

    let resources = vec![
        handle_resource("missing-handle".into()),
        handle_resource(owned.handle_id.clone()),
        handle_resource(foreign.handle_id.clone()),
        handle_resource(owned.handle_id),
    ];
    let released = runtime.release_resources(&owner, &resources);

    assert_eq!(
        released.outcomes,
        vec![
            no_op_outcome(resources[0].clone()),
            released_outcome(resources[1].clone()),
            forbidden_outcome(resources[2].clone()),
            no_op_outcome(resources[3].clone()),
        ]
    );
    assert!(runtime
        .refresh_handles(&other_client, &[foreign.handle_id])
        .is_ok());
}

#[test]
fn explicit_refresh_renews_presend_handle_lease() {
    let runtime = AttachmentRuntime::new();
    let (_files, handle) = registered_file_reference(&runtime, "task-1");
    let deadline = runtime.expire_all_at_test_deadline();

    runtime
        .refresh_handles(
            TaskId::from("task-1"),
            std::slice::from_ref(&handle.handle_id),
        )
        .unwrap();
    runtime.prune_expired_at_for_test(deadline);

    assert_eq!(
        runtime
            .refresh_handles(TaskId::from("task-1"), &[handle.handle_id])
            .unwrap()
            .attachments
            .len(),
        1
    );
}

#[test]
fn expired_presend_handles_are_not_refreshable() {
    let runtime = AttachmentRuntime::new();
    let (_files, handle) = registered_file_reference(&runtime, "task-1");
    runtime.expire_all_for_test();

    assert_eq!(
        runtime
            .refresh_handles(TaskId::from("task-1"), &[handle.handle_id])
            .unwrap_err(),
        AttachmentRuntimeError::UnknownHandle
    );
}

#[test]
fn consumed_presend_handles_cannot_be_reused() {
    let runtime = AttachmentRuntime::new();
    let (_files, handle) = registered_file_reference(&runtime, "task-1");

    runtime
        .consume_handles(
            TaskId::from("task-1"),
            std::slice::from_ref(&handle.handle_id),
        )
        .unwrap();

    assert_eq!(
        runtime
            .resolve_for_send(TaskId::from("task-1"), &[handle.handle_id])
            .unwrap_err(),
        AttachmentRuntimeError::UnknownHandle
    );
}

#[test]
fn consume_validates_all_handles_before_removing_any() {
    let runtime = AttachmentRuntime::new();
    let (_files, handle) = registered_file_reference(&runtime, "task-1");

    assert_eq!(
        runtime
            .consume_handles(
                TaskId::from("task-1"),
                &[handle.handle_id.clone(), "missing".into()],
            )
            .unwrap_err(),
        AttachmentRuntimeError::UnknownHandle
    );
    assert!(runtime
        .resolve_for_send(TaskId::from("task-1"), &[handle.handle_id])
        .is_ok());
}

#[test]
fn reserved_handles_cannot_be_released_and_commit_consumes_them() {
    let runtime = AttachmentRuntime::new();
    let (_files, handle) = registered_file_reference(&runtime, "task-1");
    let task_id = TaskId::from("task-1");
    let reservation = runtime
        .reserve_for_send(&task_id, std::slice::from_ref(&handle.handle_id))
        .unwrap();

    let resource = handle_resource(handle.handle_id.clone());
    let released = runtime.release_resources(&task_id, std::slice::from_ref(&resource));
    let resolved = reservation.commit();

    assert_eq!(released.outcomes, vec![no_op_outcome(resource)]);
    assert_eq!(resolved.chat_attachments()[0].label, "notes.md");
    assert_eq!(
        runtime
            .resolve_for_send(&task_id, &[handle.handle_id])
            .unwrap_err(),
        AttachmentRuntimeError::UnknownHandle
    );
}

#[test]
fn dropping_send_reservation_leaves_handle_available() {
    let runtime = AttachmentRuntime::new();
    let (_files, handle) = registered_file_reference(&runtime, "task-1");
    let task_id = TaskId::from("task-1");
    let reservation = runtime
        .reserve_for_send(&task_id, std::slice::from_ref(&handle.handle_id))
        .unwrap();

    drop(reservation);

    assert!(runtime
        .resolve_for_send(&task_id, &[handle.handle_id])
        .is_ok());
}

#[test]
fn expired_file_browser_entries_cannot_create_handles() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("notes.md"), "hello").unwrap();
    let runtime = AttachmentRuntime::new();
    let task_id = TaskId::from("task-1");
    let root = runtime.list_roots(&task_id, temp.path()).roots.remove(0);
    let listing = runtime
        .list_directory(&task_id, temp.path(), &root.root_id, None)
        .unwrap();
    runtime.expire_all_for_test();

    assert_eq!(
        runtime
            .create_file_reference(&task_id, &listing.entries[0].entry_id)
            .unwrap_err(),
        AttachmentRuntimeError::UnknownEntry
    );
}

#[test]
fn file_browser_hides_internal_generated_and_dependency_directories() {
    let temp = tempfile::tempdir().unwrap();
    for directory in [
        ".agents",
        ".codex",
        ".github",
        ".git",
        ".openaide-web-dev-123",
        ".qa-screenshots",
        "coverage",
        "dist",
        "node_modules",
        "qa-artifacts",
        "qa-scripts",
        "target",
        "test-results",
        "tmp",
    ] {
        std::fs::create_dir(temp.path().join(directory)).unwrap();
    }
    std::fs::create_dir(temp.path().join("src")).unwrap();
    std::fs::write(temp.path().join(".gitignore"), "target").unwrap();
    std::fs::write(temp.path().join("README.md"), "hello").unwrap();
    let runtime = AttachmentRuntime::new();
    let task_id = TaskId::from("task-1");
    let root = runtime.list_roots(&task_id, temp.path()).roots.remove(0);

    let listing = runtime
        .list_directory(&task_id, temp.path(), &root.root_id, None)
        .unwrap();

    let labels: Vec<_> = listing
        .entries
        .iter()
        .map(|entry| entry.label.as_str())
        .collect();
    assert_eq!(labels, vec!["src", ".gitignore", "README.md"]);
}

#[test]
fn expired_embedded_candidates_cannot_be_confirmed() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("notes.txt"), "hello").unwrap();
    let runtime = AttachmentRuntime::new();
    let task_id = TaskId::from("task-1");
    let root = runtime.list_roots(&task_id, temp.path()).roots.remove(0);
    let listing = runtime
        .list_directory(&task_id, temp.path(), &root.root_id, None)
        .unwrap();
    let candidate = runtime
        .create_embedded_candidate(&task_id, &listing.entries[0].entry_id)
        .unwrap()
        .candidate;
    runtime.expire_all_for_test();

    let result = runtime.confirm_embedded(&task_id, &[candidate.candidate_id]);

    assert!(result.attachments.is_empty());
    assert_eq!(result.errors.len(), 1);
}

#[test]
fn resolves_file_reference_handles_for_matching_task() {
    let runtime = AttachmentRuntime::new();
    let (files, handle) = registered_file_reference(&runtime, "task-1");

    let resolved = runtime
        .resolve_for_send(TaskId::from("task-1"), &[handle.handle_id])
        .expect("handle should resolve");

    let chat = resolved.chat_attachments();
    assert_eq!(chat[0].label, "notes.md");
    assert_eq!(
        chat[0].path.as_deref(),
        Some(files.path().join("notes.md").to_string_lossy().as_ref())
    );
    let agent = resolved.agent_attachments();
    assert_eq!(
        agent[0].path.as_deref(),
        Some(files.path().join("notes.md").to_string_lossy().as_ref())
    );
    assert_eq!(resolved.fingerprint_handles(), vec!["attachment-handle-1"]);
}

#[test]
fn rejects_unknown_wrong_task_and_duplicate_handles() {
    let runtime = AttachmentRuntime::new();
    let (_files, handle) = registered_file_reference(&runtime, "task-1");

    assert_eq!(
        runtime
            .resolve_for_send(TaskId::from("task-1"), &["missing".into()])
            .unwrap_err(),
        AttachmentRuntimeError::UnknownHandle
    );
    assert_eq!(
        runtime
            .resolve_for_send(
                TaskId::from("task-2"),
                std::slice::from_ref(&handle.handle_id)
            )
            .unwrap_err(),
        AttachmentRuntimeError::WrongTask
    );
    assert_eq!(
        runtime
            .resolve_for_send(
                TaskId::from("task-1"),
                &[handle.handle_id.clone(), handle.handle_id],
            )
            .unwrap_err(),
        AttachmentRuntimeError::DuplicateHandle
    );
}

fn registered_file_reference(
    runtime: &AttachmentRuntime,
    task_id: &str,
) -> (tempfile::TempDir, super::RegisteredAttachmentHandle) {
    let files = tempfile::tempdir().unwrap();
    let path = files.path().join("notes.md");
    std::fs::write(&path, "hello").unwrap();
    let handle = runtime.register_file_reference_for_test(TaskId::from(task_id), "notes.md", path);
    (files, handle)
}

fn handle_resource(
    id: openaide_app_server_protocol::ids::AttachmentHandleId,
) -> AttachmentResourceId {
    AttachmentResourceId::Handle { id }
}

fn candidate_resource(
    id: openaide_app_server_protocol::ids::AttachmentCandidateId,
) -> AttachmentResourceId {
    AttachmentResourceId::Candidate { id }
}

fn released_outcome(resource: AttachmentResourceId) -> AttachmentReleaseOutcome {
    AttachmentReleaseOutcome {
        resource,
        status: AttachmentReleaseStatus::Released,
    }
}

fn no_op_outcome(resource: AttachmentResourceId) -> AttachmentReleaseOutcome {
    AttachmentReleaseOutcome {
        resource,
        status: AttachmentReleaseStatus::NoOp,
    }
}

fn forbidden_outcome(resource: AttachmentResourceId) -> AttachmentReleaseOutcome {
    AttachmentReleaseOutcome {
        resource,
        status: AttachmentReleaseStatus::Forbidden,
    }
}

trait TestEntryKindSort {
    fn sort_key(self) -> u8;
}

impl TestEntryKindSort for openaide_app_server_protocol::attachment::FileBrowserEntryKind {
    fn sort_key(self) -> u8 {
        match self {
            Self::Directory => 0,
            Self::File => 1,
        }
    }
}
