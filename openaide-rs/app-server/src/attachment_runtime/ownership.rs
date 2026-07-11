use openaide_app_server_protocol::ids::{ClientInstanceId, TaskId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttachmentOwner {
    client_instance_id: ClientInstanceId,
    task_id: TaskId,
}

impl AttachmentOwner {
    pub(crate) fn new(client_instance_id: &ClientInstanceId, task_id: &TaskId) -> Self {
        Self {
            client_instance_id: client_instance_id.clone(),
            task_id: task_id.clone(),
        }
    }

    pub(super) fn belongs_to(&self, owner: &Self) -> bool {
        self == owner
    }

    pub(super) fn belongs_to_task(&self, owner: &Self) -> bool {
        self.task_id == owner.task_id
    }

    pub(super) fn belongs_to_client(&self, client_instance_id: &ClientInstanceId) -> bool {
        &self.client_instance_id == client_instance_id
    }

    #[cfg(test)]
    pub(crate) fn test_client_instance_id() -> ClientInstanceId {
        ClientInstanceId::from("attachment-test-client")
    }
}

impl From<&AttachmentOwner> for AttachmentOwner {
    fn from(owner: &AttachmentOwner) -> Self {
        owner.clone()
    }
}

#[cfg(test)]
impl From<&TaskId> for AttachmentOwner {
    fn from(task_id: &TaskId) -> Self {
        Self::new(&Self::test_client_instance_id(), task_id)
    }
}

#[cfg(test)]
impl From<TaskId> for AttachmentOwner {
    fn from(task_id: TaskId) -> Self {
        Self::from(&task_id)
    }
}
