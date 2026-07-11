use serde::Serialize;
use serde_json::json;

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeNotification {
    pub jsonrpc: &'static str,
    pub method: &'static str,
    pub params: serde_json::Value,
}

impl RuntimeNotification {
    pub fn task_updated(task_id: &str, revision: u64) -> Self {
        Self {
            jsonrpc: "2.0",
            method: "task.updated",
            params: json!({ "task_id": task_id, "revision": revision }),
        }
    }
}
