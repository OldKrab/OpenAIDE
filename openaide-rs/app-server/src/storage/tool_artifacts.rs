use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{
    ActivityStep, ActivityToolContent, ActivityToolDetails, NormalizedMessage,
};

#[cfg(test)]
use super::task_journal::{TaskWrite, ToolArtifactReplacement};
use super::Store;

#[derive(Debug, Clone)]
pub(crate) struct PersistedToolDetail {
    pub artifact_id: String,
    pub details: ActivityToolDetails,
}

impl Store {
    #[cfg(test)]
    pub(crate) fn persist_tool_artifacts(
        &self,
        task_id: &str,
        message: &mut NormalizedMessage,
    ) -> Result<Vec<PersistedToolDetail>, RuntimeError> {
        let replacements = extract_tool_artifacts(message);
        if replacements.is_empty() {
            return Ok(Vec::new());
        }
        let projection = self.task_journal().load(task_id)?;
        self.task_journal()
            .submit(TaskWrite::barrier_replace_projection(
                projection,
                replacements
                    .iter()
                    .map(|detail| ToolArtifactReplacement {
                        artifact_id: detail.artifact_id.clone(),
                        details: detail.details.clone(),
                    })
                    .collect(),
            ))?
            .wait()?;
        Ok(replacements)
    }

    pub fn read_tool_artifact(
        &self,
        task_id: &str,
        artifact_id: &str,
    ) -> Result<ActivityToolDetails, RuntimeError> {
        self.task_journal()
            .load_tool_artifact(task_id, artifact_id)?
            .details
            .ok_or_else(|| {
                RuntimeError::Storage("Tool detail has no structured baseline".to_string())
            })
    }

    pub(crate) fn read_tool_artifact_projection(
        &self,
        task_id: &str,
        artifact_id: &str,
    ) -> Result<super::task_journal::ToolArtifactProjection, RuntimeError> {
        self.task_journal().load_tool_artifact(task_id, artifact_id)
    }
}

/// Removes heavy Tool details from Chat and returns their lazy replacements.
pub(crate) fn extract_tool_artifacts(message: &mut NormalizedMessage) -> Vec<PersistedToolDetail> {
    let NormalizedMessage::Activity { id, steps, .. } = message else {
        return Vec::new();
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
        *detail_artifact_id = Some(artifact_id.clone());
        persisted.push(PersistedToolDetail {
            artifact_id,
            details,
        });
    }
    persisted
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

/// Stable opaque identity. Two independently seeded FNV streams keep raw
/// Native Session/tool identifiers out of storage paths and diagnostics.
pub(crate) fn tool_artifact_id(message_id: &str, step_index: usize) -> String {
    let input = format!("{message_id}\0{step_index}");
    let left = fnv64(input.as_bytes(), 0xcbf29ce484222325);
    let right = fnv64(input.as_bytes(), 0x84222325cbf29ce4);
    format!("artifact_{left:016x}{right:016x}")
}

fn fnv64(bytes: &[u8], seed: u64) -> u64 {
    bytes.iter().fold(seed, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}
