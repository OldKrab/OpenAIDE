use crate::protocol::errors::RuntimeError;

pub fn validate_task_id(task_id: &str) -> Result<(), RuntimeError> {
    let valid = !task_id.is_empty()
        && task_id.len() <= 96
        && task_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-');

    if valid {
        Ok(())
    } else {
        Err(RuntimeError::InvalidParams("task_id".to_string()))
    }
}
