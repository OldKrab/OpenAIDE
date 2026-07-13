use std::fs;

use serde::{Deserialize, Serialize};

use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{
    ActivityStep, ActivityToolContent, ActivityToolDetails, NormalizedMessage,
};

use super::atomic;
use super::Store;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolDetailArtifact {
    pub task_id: String,
    pub artifact_id: String,
    pub details: ActivityToolDetails,
}

#[derive(Debug, Clone)]
pub(crate) struct PersistedToolDetail {
    pub artifact_id: String,
    pub details: ActivityToolDetails,
}

impl Store {
    pub(crate) fn persist_tool_artifacts(
        &self,
        task_id: &str,
        message: &mut NormalizedMessage,
    ) -> Result<Vec<PersistedToolDetail>, RuntimeError> {
        let NormalizedMessage::Activity { id, steps, .. } = message else {
            return Ok(Vec::new());
        };

        let mut persisted = Vec::new();

        for (index, step) in steps.iter_mut().enumerate() {
            let ActivityStep::Tool {
                name,
                input_summary,
                detail_artifact_id,
                details,
                ..
            } = step
            else {
                continue;
            };
            let Some(details) = details.take() else {
                continue;
            };
            if should_replace_input_summary(name, input_summary.as_deref()) {
                *input_summary = lightweight_detail_summary(&details);
            }
            let artifact_id = detail_artifact_id
                .clone()
                .unwrap_or_else(|| tool_artifact_id(id, index));
            let details = *details;
            self.write_tool_artifact(task_id, &artifact_id, details.clone())?;
            *detail_artifact_id = Some(artifact_id.clone());
            persisted.push(PersistedToolDetail {
                artifact_id,
                details,
            });
        }

        Ok(persisted)
    }

    pub fn read_tool_artifact(
        &self,
        task_id: &str,
        artifact_id: &str,
    ) -> Result<ActivityToolDetails, RuntimeError> {
        validate_artifact_id(artifact_id)?;
        let path = self
            .tool_artifact_dir(task_id)?
            .join(format!("{artifact_id}.json"));
        let text = fs::read_to_string(path)?;
        let artifact: ToolDetailArtifact = serde_json::from_str(&text)?;
        if artifact.task_id != task_id || artifact.artifact_id != artifact_id {
            return Err(RuntimeError::Storage(
                "tool artifact identity mismatch".to_string(),
            ));
        }
        Ok(artifact.details)
    }

    fn write_tool_artifact(
        &self,
        task_id: &str,
        artifact_id: &str,
        details: ActivityToolDetails,
    ) -> Result<(), RuntimeError> {
        validate_artifact_id(artifact_id)?;
        let dir = self.tool_artifact_dir(task_id)?;
        fs::create_dir_all(&dir)?;
        let artifact = ToolDetailArtifact {
            task_id: task_id.to_string(),
            artifact_id: artifact_id.to_string(),
            details,
        };
        let bytes = serde_json::to_vec_pretty(&artifact)?;
        atomic::write_bytes(&dir.join(format!("{artifact_id}.json")), &bytes)
    }

    fn tool_artifact_dir(&self, task_id: &str) -> Result<std::path::PathBuf, RuntimeError> {
        Ok(self.task_dir(task_id)?.join("tool-artifacts"))
    }
}

pub(super) fn should_replace_input_summary(tool_name: &str, summary: Option<&str>) -> bool {
    let Some(summary) = summary.map(str::trim) else {
        return true;
    };
    if summary.is_empty() {
        return true;
    }
    let normalized = summary.to_ascii_lowercase();
    matches!(
        (tool_name, normalized.as_str()),
        ("edit", "edit")
            | ("edit", "updated file")
            | ("edit", "created file")
            | ("read", "read")
            | ("read", "read file")
            | ("delete", "delete")
            | ("delete", "deleted file")
            | ("move", "move")
            | ("move", "moved file")
    )
}

pub(super) fn lightweight_detail_summary(details: &ActivityToolDetails) -> Option<String> {
    let path = details
        .locations
        .first()
        .map(|location| location.path.as_str())
        .or_else(|| first_diff_path(&details.content))
        .or(details
            .input
            .as_ref()
            .and_then(|input| input.path.as_deref()))?;
    Some(path_leaf(path))
}

fn first_diff_path(content: &[ActivityToolContent]) -> Option<&str> {
    content.iter().find_map(|content| match content {
        ActivityToolContent::Diff { path, .. } => Some(path.as_str()),
        _ => None,
    })
}

fn path_leaf(value: &str) -> String {
    value
        .trim()
        .trim_matches(|ch| matches!(ch, '\'' | '"' | '`'))
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .find(|part| !part.is_empty())
        .unwrap_or(value)
        .to_string()
}

fn tool_artifact_id(message_id: &str, step_index: usize) -> String {
    let mut value = message_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    value.truncate(96);
    format!("{value}_{step_index}")
}

fn validate_artifact_id(value: &str) -> Result<(), RuntimeError> {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Ok(());
    }
    Err(RuntimeError::InvalidParams("artifact_id".to_string()))
}
