use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::ClientInstanceId;
use openaide_app_server_protocol::task::{
    ActivityToolContent, ActivityToolField, ActivityToolInput, ActivityToolLocation,
    ActivityToolOutput, TaskToolDetailParams, TaskToolDetailResult,
};

use super::{protocol_error_from_runtime, TaskProductApi};

pub(crate) trait TaskToolDetailWorkflow: Send + Sync {
    fn tool_detail_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskToolDetailParams,
    ) -> Result<TaskToolDetailResult, ProtocolError>;
}

impl TaskToolDetailWorkflow for TaskProductApi {
    fn tool_detail_for_client(
        &self,
        client_instance_id: &ClientInstanceId,
        params: TaskToolDetailParams,
    ) -> Result<TaskToolDetailResult, ProtocolError> {
        self.read_task_for_client(params.task_id.as_str(), client_instance_id)?;
        let details = self
            .store
            .read_tool_artifact(params.task_id.as_str(), &params.artifact_id)
            .map_err(protocol_error_from_runtime)?;
        Ok(map_tool_details(details))
    }
}

fn map_tool_details(details: crate::protocol::model::ActivityToolDetails) -> TaskToolDetailResult {
    TaskToolDetailResult {
        locations: details.locations.into_iter().map(map_location).collect(),
        content: details.content.into_iter().map(map_content).collect(),
        input: details.input.map(map_input),
        output: details.output.map(map_output),
    }
}

fn map_location(location: crate::protocol::model::ActivityToolLocation) -> ActivityToolLocation {
    ActivityToolLocation {
        path: location.path,
        line: location.line,
    }
}

fn map_content(content: crate::protocol::model::ActivityToolContent) -> ActivityToolContent {
    match content {
        crate::protocol::model::ActivityToolContent::Text { text } => {
            ActivityToolContent::Text { text }
        }
        crate::protocol::model::ActivityToolContent::Diff {
            path,
            old_text,
            new_text,
        } => ActivityToolContent::Diff {
            path,
            old_text,
            new_text,
        },
        crate::protocol::model::ActivityToolContent::Terminal { terminal_id } => {
            ActivityToolContent::Terminal { terminal_id }
        }
        crate::protocol::model::ActivityToolContent::Other { label } => {
            ActivityToolContent::Other { label }
        }
    }
}

fn map_input(input: crate::protocol::model::ActivityToolInput) -> ActivityToolInput {
    ActivityToolInput {
        command: input.command,
        cwd: input.cwd,
        query: input.query,
        queries: if input.queries.is_empty() {
            None
        } else {
            Some(input.queries)
        },
        url: input.url,
        path: input.path,
        fields: input.fields.into_iter().map(map_field).collect(),
    }
}

fn map_output(output: crate::protocol::model::ActivityToolOutput) -> ActivityToolOutput {
    ActivityToolOutput {
        stdout: output.stdout,
        stderr: output.stderr,
        formatted_output: output.formatted_output,
        aggregated_output: output.aggregated_output,
        exit_code: output.exit_code,
        success: output.success,
        fields: output.fields.into_iter().map(map_field).collect(),
    }
}

fn map_field(field: crate::protocol::model::ActivityToolField) -> ActivityToolField {
    ActivityToolField {
        name: field.name,
        value: field.value,
    }
}
