use serde::Serialize;

use super::model::{TaskSnapshot, TaskSummary};

#[derive(Debug, Serialize)]
pub struct HealthResult {
    pub status: &'static str,
    pub version: String,
    pub methods: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct TaskListResult {
    pub tasks: Vec<TaskSummary>,
    pub revision: u64,
    pub archived: bool,
}

#[derive(Debug, Serialize)]
pub struct EmptyResult {}

#[derive(Debug, Serialize)]
pub struct SnapshotResult {
    #[serde(flatten)]
    pub snapshot: TaskSnapshot,
}
