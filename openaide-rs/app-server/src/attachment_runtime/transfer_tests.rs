use openaide_app_server_protocol::attachment::{AttachmentTransferStatus, PreSendAttachment};
use openaide_app_server_protocol::ids::{AttachmentHandleId, ClientInstanceId, TaskId};

use super::super::{AttachmentOwner, AttachmentRuntime, AttachmentRuntimeError};

fn pasted(runtime: &AttachmentRuntime, owner: &AttachmentOwner) -> PreSendAttachment {
    runtime
        .create_pasted_image(owner, "/tmp/Screenshot.png", "image/png", "aW1hZ2U=")
        .unwrap()
        .attachment
}

fn statuses(
    runtime: &AttachmentRuntime,
    from: &AttachmentOwner,
    to: &AttachmentOwner,
    handles: &[AttachmentHandleId],
) -> Vec<AttachmentTransferStatus> {
    runtime
        .transfer_handles(from, to, handles)
        .outcomes
        .into_iter()
        .map(|outcome| outcome.status)
        .collect()
}

#[test]
fn transfer_moves_a_handle_to_the_successor_task_with_its_id_and_content() {
    let runtime = AttachmentRuntime::new();
    let old = AttachmentOwner::from(TaskId::from("task-old"));
    let new = AttachmentOwner::from(TaskId::from("task-new"));
    let image = pasted(&runtime, &old);

    let handles = [image.handle_id.clone()];
    assert_eq!(
        statuses(&runtime, &old, &new, &handles),
        [AttachmentTransferStatus::Transferred]
    );

    let resolved = runtime.resolve_for_send(&new, &handles).unwrap();
    assert_eq!(resolved.chat_attachments()[0].kind, "image");
    assert_eq!(
        runtime.resolve_for_send(&old, &handles).unwrap_err(),
        AttachmentRuntimeError::WrongTask
    );
}

#[test]
fn transfer_is_idempotent_for_a_handle_already_moved() {
    let runtime = AttachmentRuntime::new();
    let old = AttachmentOwner::from(TaskId::from("task-old"));
    let new = AttachmentOwner::from(TaskId::from("task-new"));
    let image = pasted(&runtime, &old);
    let handles = [image.handle_id.clone(), image.handle_id];

    assert_eq!(
        statuses(&runtime, &old, &new, &handles),
        [
            AttachmentTransferStatus::Transferred,
            AttachmentTransferStatus::Transferred
        ]
    );
}

#[test]
fn transfer_refuses_another_clients_or_another_tasks_handle() {
    let runtime = AttachmentRuntime::new();
    let old = AttachmentOwner::from(TaskId::from("task-old"));
    let new = AttachmentOwner::from(TaskId::from("task-new"));
    let other_task = AttachmentOwner::from(TaskId::from("task-other"));
    let other_client = AttachmentOwner::new(&ClientInstanceId::from("other-client"), &TaskId::from("task-old"));
    let unrelated = pasted(&runtime, &other_task);
    let foreign = pasted(&runtime, &other_client);

    assert_eq!(
        statuses(&runtime, &old, &new, &[unrelated.handle_id.clone(), foreign.handle_id.clone()]),
        [
            AttachmentTransferStatus::Forbidden,
            AttachmentTransferStatus::Forbidden
        ]
    );
    assert!(runtime.resolve_for_send(&other_task, &[unrelated.handle_id]).is_ok());
    assert!(runtime.resolve_for_send(&other_client, &[foreign.handle_id]).is_ok());
}

#[test]
fn transfer_reports_released_expired_and_reserved_handles_missing() {
    let runtime = AttachmentRuntime::new();
    let old = AttachmentOwner::from(TaskId::from("task-old"));
    let new = AttachmentOwner::from(TaskId::from("task-new"));
    let reserved = pasted(&runtime, &old);
    let _reservation = runtime
        .reserve_for_send(&old, std::slice::from_ref(&reserved.handle_id))
        .unwrap();

    assert_eq!(
        statuses(
            &runtime,
            &old,
            &new,
            &[reserved.handle_id, AttachmentHandleId::from("attachment-handle-unknown")],
        ),
        [
            AttachmentTransferStatus::Missing,
            AttachmentTransferStatus::Missing
        ]
    );

    let expired = pasted(&runtime, &old);
    runtime.expire_all_for_test();
    assert_eq!(
        statuses(&runtime, &old, &new, &[expired.handle_id]),
        [AttachmentTransferStatus::Missing]
    );
}
