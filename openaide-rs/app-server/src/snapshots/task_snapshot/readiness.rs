use openaide_app_server_protocol::errors::{ProtocolError, ProtocolErrorCode};
use openaide_app_server_protocol::ids::{AgentConfigOptionId, ClientMutationId};
use openaide_app_server_protocol::snapshot::{
    AgentConfigOptionCurrentValue, AgentConfigOptionKind, AgentConfigOptionSnapshot,
    AgentConfigOptionValueSnapshot, AgentSlashCommandInputSnapshot, AgentSlashCommandSnapshot,
    LiveSessionDataState, PendingAgentConfigChange, TaskAgentCommandsSnapshot,
    TaskAgentConfigSnapshot, TaskPreparationAction, TaskPreparationSnapshot, TaskPreparationStep,
    TaskPreparationStepKind, TaskPreparationStepStatus, TaskSendBlocker, TaskSendBlockerKind,
    TaskSendCapabilitySnapshot, TaskSendCapabilityState, TaskSetupBlocker, TaskSetupBlockerKind,
};

use crate::protocol::model::{
    AgentCommand, ConfigOption, ConfigOptionCategory, ConfigOptionCurrentValue, ConfigOptionKind,
    ConfigOptionValue, TaskSnapshot as StoredTaskSnapshot, TaskStatus as LegacyTaskStatus,
};
use crate::storage::records::{TaskPreparationBlockerRecord, TaskPreparationRecord};

pub(super) fn preparation_snapshot(preparation: &TaskPreparationRecord) -> TaskPreparationSnapshot {
    match preparation {
        TaskPreparationRecord::Needed | TaskPreparationRecord::Preparing => {
            TaskPreparationSnapshot::Preparing {
                steps: vec![
                    TaskPreparationStep {
                        kind: TaskPreparationStepKind::CreatingNativeSession,
                        status: TaskPreparationStepStatus::Running,
                        label: "Creating Agent session".to_string(),
                    },
                    TaskPreparationStep {
                        kind: TaskPreparationStepKind::LoadingConfigOptions,
                        status: TaskPreparationStepStatus::Pending,
                        label: "Loading Agent options".to_string(),
                    },
                ],
            }
        }
        TaskPreparationRecord::Ready => TaskPreparationSnapshot::Ready,
        TaskPreparationRecord::Blocked { reason, message } => TaskPreparationSnapshot::Blocked {
            blocker: TaskSetupBlocker {
                kind: setup_blocker_kind(*reason),
                message: message.clone(),
            },
            actions: setup_blocker_actions(*reason),
        },
        TaskPreparationRecord::Failed {
            message,
            native_session_missing,
        } => TaskPreparationSnapshot::Failed {
            error: preparation_error(message, *native_session_missing),
            actions: vec![TaskPreparationAction::Retry, TaskPreparationAction::Discard],
        },
    }
}

pub(super) fn agent_config_snapshot(snapshot: &StoredTaskSnapshot) -> TaskAgentConfigSnapshot {
    match &snapshot.preparation {
        TaskPreparationRecord::Needed | TaskPreparationRecord::Preparing => {
            TaskAgentConfigSnapshot {
                state: LiveSessionDataState::Loading,
                options: Vec::new(),
                pending_change: None,
                error: None,
            }
        }
        TaskPreparationRecord::Ready => match &snapshot.config_options_catalog {
            Some(_) => TaskAgentConfigSnapshot {
                state: LiveSessionDataState::Ready,
                options: agent_config_options(snapshot),
                pending_change: pending_config_change(snapshot),
                error: None,
            },
            None => TaskAgentConfigSnapshot {
                state: LiveSessionDataState::Unavailable,
                options: Vec::new(),
                pending_change: pending_config_change(snapshot),
                error: None,
            },
        },
        TaskPreparationRecord::Blocked { reason, message } => TaskAgentConfigSnapshot {
            state: LiveSessionDataState::Unavailable,
            options: Vec::new(),
            pending_change: None,
            error: Some(setup_blocker_error(*reason, message)),
        },
        TaskPreparationRecord::Failed { message, .. } => TaskAgentConfigSnapshot {
            state: LiveSessionDataState::Failed,
            options: Vec::new(),
            pending_change: None,
            error: Some(preparation_error(message, false)),
        },
    }
}

fn pending_config_change(snapshot: &StoredTaskSnapshot) -> Option<PendingAgentConfigChange> {
    snapshot
        .pending_config_change
        .as_ref()
        .map(|pending| PendingAgentConfigChange {
            client_mutation_id: ClientMutationId::from(pending.client_mutation_id.clone()),
            config_id: AgentConfigOptionId::from(pending.config_id.clone()),
            requested_value: project_current_value(&pending.requested_value),
        })
}

fn agent_config_options(snapshot: &StoredTaskSnapshot) -> Vec<AgentConfigOptionSnapshot> {
    snapshot
        .config_options_catalog
        .as_ref()
        .into_iter()
        .flat_map(|catalog| catalog.options.iter().map(project_config_option))
        .collect()
}

fn project_config_option(option: &ConfigOption) -> AgentConfigOptionSnapshot {
    AgentConfigOptionSnapshot {
        config_id: option.id.as_str().into(),
        label: option.label.clone(),
        description: option.description.clone(),
        category: option.category.as_ref().map(config_category),
        kind: match option.kind {
            ConfigOptionKind::Select => AgentConfigOptionKind::Select,
            ConfigOptionKind::Boolean => AgentConfigOptionKind::Boolean,
        },
        current_value: project_current_value(&option.current_value),
        values: option.values.iter().map(project_config_value).collect(),
    }
}

fn project_current_value(value: &ConfigOptionCurrentValue) -> AgentConfigOptionCurrentValue {
    match value {
        ConfigOptionCurrentValue::Id { value } => AgentConfigOptionCurrentValue::Id {
            value: value.clone(),
        },
        ConfigOptionCurrentValue::Boolean { value } => {
            AgentConfigOptionCurrentValue::Boolean { value: *value }
        }
    }
}

fn project_config_value(value: &ConfigOptionValue) -> AgentConfigOptionValueSnapshot {
    AgentConfigOptionValueSnapshot {
        value: value.id.clone(),
        label: value.label.clone(),
        description: value.description.clone(),
    }
}

fn config_category(category: &ConfigOptionCategory) -> String {
    match category {
        ConfigOptionCategory::Mode => "mode",
        ConfigOptionCategory::Model => "model",
        ConfigOptionCategory::ThoughtLevel => "thoughtLevel",
        ConfigOptionCategory::Other => "other",
    }
    .to_string()
}

pub(super) fn agent_commands_snapshot(snapshot: &StoredTaskSnapshot) -> TaskAgentCommandsSnapshot {
    match &snapshot.preparation {
        TaskPreparationRecord::Needed | TaskPreparationRecord::Preparing => {
            TaskAgentCommandsSnapshot {
                state: LiveSessionDataState::Loading,
                commands: Vec::new(),
                error: None,
            }
        }
        TaskPreparationRecord::Ready => TaskAgentCommandsSnapshot {
            state: match &snapshot.agent_commands_catalog {
                Some(_) => LiveSessionDataState::Ready,
                None => LiveSessionDataState::Unavailable,
            },
            commands: snapshot
                .agent_commands_catalog
                .as_ref()
                .map(|catalog| catalog.commands.iter().map(project_command).collect())
                .unwrap_or_default(),
            error: None,
        },
        TaskPreparationRecord::Blocked { reason, message } => TaskAgentCommandsSnapshot {
            state: LiveSessionDataState::Unavailable,
            commands: Vec::new(),
            error: Some(setup_blocker_error(*reason, message)),
        },
        TaskPreparationRecord::Failed { message, .. } => TaskAgentCommandsSnapshot {
            state: LiveSessionDataState::Failed,
            commands: Vec::new(),
            error: Some(preparation_error(message, false)),
        },
    }
}

fn project_command(command: &AgentCommand) -> AgentSlashCommandSnapshot {
    AgentSlashCommandSnapshot {
        name: command.name.clone(),
        description: command.description.clone(),
        input: command
            .input_hint
            .as_ref()
            .map(|hint| AgentSlashCommandInputSnapshot { hint: hint.clone() }),
    }
}

pub(super) fn send_capability_for_task(
    status: LegacyTaskStatus,
    preparation: &TaskPreparationRecord,
) -> TaskSendCapabilitySnapshot {
    match preparation {
        TaskPreparationRecord::Needed | TaskPreparationRecord::Preparing => {
            return TaskSendCapabilitySnapshot {
                state: TaskSendCapabilityState::Loading,
                blockers: vec![TaskSendBlocker {
                    kind: TaskSendBlockerKind::TaskPreparing,
                    message: "Task Agent preparation is still running".to_string(),
                }],
            };
        }
        TaskPreparationRecord::Failed { message, .. } => {
            return TaskSendCapabilitySnapshot {
                state: TaskSendCapabilityState::Failed,
                blockers: vec![TaskSendBlocker {
                    kind: TaskSendBlockerKind::FailedValidation,
                    message: format!("Task Agent preparation failed: {message}"),
                }],
            };
        }
        TaskPreparationRecord::Blocked { message, .. } => {
            return TaskSendCapabilitySnapshot {
                state: TaskSendCapabilityState::Blocked,
                blockers: vec![TaskSendBlocker {
                    kind: TaskSendBlockerKind::FailedValidation,
                    message: message.clone(),
                }],
            };
        }
        TaskPreparationRecord::Ready => {}
    }
    match status {
        LegacyTaskStatus::Inactive | LegacyTaskStatus::Completed | LegacyTaskStatus::Active => {
            TaskSendCapabilitySnapshot {
                state: TaskSendCapabilityState::Ready,
                blockers: Vec::new(),
            }
        }
        LegacyTaskStatus::Starting | LegacyTaskStatus::Stopping | LegacyTaskStatus::Waiting => {
            TaskSendCapabilitySnapshot {
                state: TaskSendCapabilityState::Blocked,
                blockers: vec![TaskSendBlocker {
                    kind: TaskSendBlockerKind::TaskRunning,
                    message: "Task is already running".to_string(),
                }],
            }
        }
        LegacyTaskStatus::Failed => TaskSendCapabilitySnapshot {
            state: TaskSendCapabilityState::Ready,
            blockers: Vec::new(),
        },
    }
}

fn preparation_error(message: &str, native_session_missing: bool) -> ProtocolError {
    ProtocolError {
        code: if native_session_missing {
            ProtocolErrorCode::NotFound
        } else {
            ProtocolErrorCode::Internal
        },
        message: message.to_string(),
        recoverable: true,
        target: None,
    }
}

fn setup_blocker_kind(reason: TaskPreparationBlockerRecord) -> TaskSetupBlockerKind {
    match reason {
        TaskPreparationBlockerRecord::AuthRequired => TaskSetupBlockerKind::AuthRequired,
        TaskPreparationBlockerRecord::SetupRequired => TaskSetupBlockerKind::SetupRequired,
        TaskPreparationBlockerRecord::NodeJsRequired => TaskSetupBlockerKind::NodeJsRequired,
    }
}

fn setup_blocker_actions(reason: TaskPreparationBlockerRecord) -> Vec<TaskPreparationAction> {
    match reason {
        TaskPreparationBlockerRecord::AuthRequired => vec![
            TaskPreparationAction::Authenticate,
            TaskPreparationAction::OpenAgentSettings,
        ],
        TaskPreparationBlockerRecord::SetupRequired
        | TaskPreparationBlockerRecord::NodeJsRequired => vec![
            TaskPreparationAction::Retry,
            TaskPreparationAction::OpenAgentSettings,
        ],
    }
}

fn setup_blocker_error(reason: TaskPreparationBlockerRecord, message: &str) -> ProtocolError {
    ProtocolError {
        code: match reason {
            TaskPreparationBlockerRecord::AuthRequired => ProtocolErrorCode::Unauthorized,
            TaskPreparationBlockerRecord::SetupRequired => ProtocolErrorCode::CapabilityUnavailable,
            TaskPreparationBlockerRecord::NodeJsRequired => ProtocolErrorCode::NodeJsRequired,
        },
        message: message.to_string(),
        recoverable: true,
        target: None,
    }
}
