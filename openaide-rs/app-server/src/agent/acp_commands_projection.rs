use crate::agent::acp_schema::{AvailableCommand, AvailableCommandInput, AvailableCommandsUpdate};

use crate::protocol::model::{AgentCommand, AgentCommandsCatalog};

pub(super) fn normalize_available_commands(
    update: AvailableCommandsUpdate,
) -> AgentCommandsCatalog {
    AgentCommandsCatalog {
        commands: update
            .available_commands
            .into_iter()
            .map(normalize_available_command)
            .collect(),
    }
}

fn normalize_available_command(command: AvailableCommand) -> AgentCommand {
    AgentCommand {
        name: command.name,
        description: command.description,
        input_hint: command.input.and_then(command_input_hint),
    }
}

fn command_input_hint(input: AvailableCommandInput) -> Option<String> {
    match input {
        AvailableCommandInput::Unstructured(input) => Some(input.hint),
        _ => None,
    }
}
