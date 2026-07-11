use std::collections::HashMap;

use openaide_app_server::agent::{AcpTraceStatus, RuntimeDeveloperSettings, RuntimeSettings};
use openaide_app_server::diagnostics::{RuntimeDiagnostics, TaskDiagnostics};
use openaide_app_server::protocol::host::HostRequest;
use openaide_app_server::protocol::jsonrpc::RpcId;
use openaide_app_server::protocol::model::{
    ActivityStatus, ActivityStep, ActivityToolContent, ActivityToolDetails, ActivityToolField,
    ActivityToolInput, ActivityToolLocation, ActivityToolOutput, AgentAuthMethodSummary,
    AgentAuthenticateResult, AgentAuthenticateStatus, AgentListSessionsResult, AgentListedSession,
    AgentProbeCapabilities, AgentProbeResult, AgentProbeStatus, Attachment, ChatMessage,
    InterruptionReason, IsolationKind, MessagePage, NormalizedMessage, PermissionDecision,
    PermissionOption, PermissionOptionKind, PermissionState, PermissionToolCall, SettingsSummary,
    TaskSnapshot, TaskStatus, TaskSummary,
};
use openaide_app_server::protocol::notifications::RuntimeNotification;
use openaide_app_server::protocol::params::{
    AgentAuthenticateParams, AgentConfigOptionsParams, AgentListSessionsParams, AgentProbeParams,
    ChatPageParams, ChatTailParams, DeleteMode, PermissionRespondParams,
    RuntimeAcpTraceSettingsPatch, RuntimeDeveloperSettingsPatch, RuntimeUpdateSettingsParams,
    SessionPromptParams, SessionSetConfigOptionParams, TaskCreateMode, TaskCreateParams,
    TaskDeleteParams, TaskIdParams, TaskListParams, TaskSnapshotParams, ToolDetailParams,
};
use openaide_app_server::protocol::results::{HealthResult, TaskListResult};
use openaide_app_server::storage::records::TaskPreparationRecord;
use serde_json::{json, Value};

fn main() {
    let task = task_summary();
    let tool_details = tool_details();
    let chat = MessagePage {
        task_id: task.task_id.clone(),
        items: vec![
            chat_message(NormalizedMessage::User {
                id: "msg_user".to_string(),
                text: "Update docs".to_string(),
                created_at: "2026-05-22T00:00:00Z".to_string(),
                attachments: vec![attachment()],
            }),
            chat_message(NormalizedMessage::AgentText {
                id: "msg_agent".to_string(),
                text: "Done.".to_string(),
                created_at: "2026-05-22T00:00:01Z".to_string(),
                streaming: true,
            }),
            chat_message(NormalizedMessage::Activity {
                id: "activity_1".to_string(),
                title: "Read file".to_string(),
                status: ActivityStatus::Running,
                created_at: "2026-05-22T00:00:02Z".to_string(),
                collapsed: true,
                steps: vec![
                    ActivityStep::Text {
                        text: "Started".to_string(),
                        level: Some("info".to_string()),
                    },
                    ActivityStep::Tool {
                        tool_call_id: Some("tool_read_1".to_string()),
                        name: "read".to_string(),
                        status: ActivityStatus::Completed,
                        input_summary: Some("README.md".to_string()),
                        output_preview: Some("content".to_string()),
                        detail_artifact_id: Some("artifact_1".to_string()),
                        details: Some(Box::new(tool_details.clone())),
                    },
                    ActivityStep::Command {
                        command_label: "cargo test".to_string(),
                        status: ActivityStatus::Error,
                        exit_code: Some(101),
                        output_preview: Some("failed".to_string()),
                    },
                ],
            }),
            chat_message(NormalizedMessage::Permission {
                id: "perm_1".to_string(),
                request_id: "request_1".to_string(),
                app_server_request_id: None,
                title: "Allow edit".to_string(),
                description: Some("Edit README.md".to_string()),
                scope: Some("workspace".to_string()),
                risk: Some("write".to_string()),
                tool_call: PermissionToolCall {
                    id: "tool_1".to_string(),
                    title: "Edit file".to_string(),
                    kind: Some("edit".to_string()),
                },
                state: PermissionState::Pending,
                created_at: "2026-05-22T00:00:03Z".to_string(),
                options: vec![PermissionOption {
                    id: "allow_once".to_string(),
                    label: "Allow once".to_string(),
                    kind: Some(PermissionOptionKind::Allow),
                    description: Some("Only this request".to_string()),
                }],
                selected_option: Some("allow_once".to_string()),
                decision: Some(PermissionDecision::Approved),
            }),
            chat_message(NormalizedMessage::Interruption {
                id: "interrupt_1".to_string(),
                reason: InterruptionReason::Canceled,
                message: "Task was stopped.".to_string(),
                created_at: "2026-05-22T00:00:04Z".to_string(),
                recoverable: true,
            }),
        ],
        has_before: true,
        total_count: 5,
        version: 6,
        start_cursor: Some("cursor_1".to_string()),
        end_cursor: Some("cursor_5".to_string()),
    };

    let snapshot = TaskSnapshot {
        task: task.clone(),
        chat: chat.clone(),
        permissions: vec![chat.items[3].message.clone()],
        settings_summary: SettingsSummary {
            agent_id: "codex".to_string(),
            isolation: IsolationKind::Local,
            model_id: Some("gpt-5.5".to_string()),
            config_options: HashMap::from([("model".to_string(), "gpt-5.5".to_string())]),
        },
        config_options_catalog: None,
        agent_commands_catalog: None,
        preparation: TaskPreparationRecord::Ready,
        revision: 9,
    };

    let output = json!({
        "params": {
            "task_create": to_value(TaskCreateParams {
                mode: TaskCreateMode::PromptStart,
                title: String::new(),
                workspace_root: "/workspace/app".to_string(),
                selected_agent_id: "codex".to_string(),
                selected_agent_label: Some("Codex".to_string()),
                selected_isolation: IsolationKind::GitWorktree,
                prompt_text: Some("Update docs".to_string()),
                external_session_id: None,
                model_id: None,
                config_options: Some(json!({ "model": "gpt-5.5" })),
                context: vec![attachment()],
            }),
            "task_id": to_value(TaskIdParams { task_id: "task_1".to_string() }),
            "task_list": to_value(TaskListParams { archived: true }),
            "task_snapshot": to_value(TaskSnapshotParams { task_id: "task_1".to_string(), tail_limit: 50 }),
            "chat_tail": to_value(ChatTailParams { task_id: "task_1".to_string(), limit: 25 }),
            "chat_page": to_value(ChatPageParams { task_id: "task_1".to_string(), before_cursor: "cursor_10".to_string(), limit: 25 }),
            "tool_detail": to_value(ToolDetailParams { task_id: "task_1".to_string(), artifact_id: "artifact_1".to_string() }),
            "session_prompt": to_value(SessionPromptParams {
                task_id: "task_1".to_string(),
                text: "Follow up".to_string(),
                prompt_attachments: vec![attachment()],
                message_id: Some("client_msg_1".to_string()),
            }),
            "task_delete": to_value(TaskDeleteParams { task_id: "task_1".to_string(), mode: DeleteMode::Delete }),
            "permission_respond": to_value(PermissionRespondParams {
                task_id: "task_1".to_string(),
                request_id: "request_1".to_string(),
                decision: PermissionDecision::Denied,
                option_id: "deny_once".to_string(),
            }),
            "runtime_update_settings": to_value(RuntimeUpdateSettingsParams {
                developer: RuntimeDeveloperSettingsPatch {
                    acp_trace: RuntimeAcpTraceSettingsPatch { enabled: Some(true) },
                },
            }),
            "agent_config_options": to_value(AgentConfigOptionsParams { agent_id: "codex".to_string(), workspace_root: "/workspace/app".to_string() }),
            "agent_probe": to_value(AgentProbeParams { agent_id: "codex".to_string() }),
            "agent_authenticate": to_value(AgentAuthenticateParams { agent_id: "codex".to_string(), method_id: "codex-login".to_string() }),
            "agent_list_sessions": to_value(AgentListSessionsParams { agent_id: "codex".to_string(), workspace_root: "/workspace/app".to_string(), cursor: Some("cursor_1".to_string()) }),
            "session_set_config_option": to_value(SessionSetConfigOptionParams {
                agent_id: "codex".to_string(),
                workspace_root: "/workspace/app".to_string(),
                config_id: "model".to_string(),
                value: "gpt-5.5".to_string(),
            }),
        },
        "results": {
            "health": to_value(HealthResult {
                status: "ok",
                version: "0.1.0".to_string(),
                methods: vec!["runtime.health", "task.create"],
            }),
            "task_list": to_value(TaskListResult {
                tasks: vec![task.clone()],
                revision: 9,
                archived: false,
            }),
            "task_snapshot": to_value(snapshot),
            "message_page": to_value(chat),
            "tool_detail": to_value(tool_details),
            "agent_probe": to_value(AgentProbeResult {
                agent_id: "codex".to_string(),
                status: AgentProbeStatus::Ready,
                protocol_version: "1".to_string(),
                implementation_name: Some("Codex ACP".to_string()),
                implementation_version: Some("1.2.3".to_string()),
                capabilities: vec!["Basic sessions".to_string(), "Delete sessions".to_string()],
                typed_capabilities: AgentProbeCapabilities {
                    resume_sessions: true,
                    delete_sessions: true,
                },
                auth_methods: vec![AgentAuthMethodSummary {
                    id: "codex-login".to_string(),
                    label: "Codex login".to_string(),
                    kind: "agent".to_string(),
                    description: Some("Sign in".to_string()),
                }],
            }),
            "agent_authenticate": to_value(AgentAuthenticateResult {
                agent_id: "codex".to_string(),
                method_id: "codex-login".to_string(),
                status: AgentAuthenticateStatus::Authenticated,
            }),
            "agent_list_sessions": to_value(AgentListSessionsResult {
                agent_id: "codex".to_string(),
                sessions: vec![AgentListedSession {
                    session_id: "session_1".to_string(),
                    cwd: "/workspace/app".to_string(),
                    title: Some("External task".to_string()),
                    last_activity: Some("2026-05-22T00:00:05Z".to_string()),
                    updated_at: Some("2026-05-22T00:00:05Z".to_string()),
                }],
                next_cursor: Some("cursor_2".to_string()),
            }),
            "runtime_diagnostics": to_value(RuntimeDiagnostics {
                status: "ready",
                version: "0.1.0".to_string(),
                method_count: 22,
                tasks: TaskDiagnostics {
                    visible_count: 1,
                    total_count: 2,
                    active_count: 1,
                    active_tasks: Vec::new(),
                    revision: 9,
                },
                redaction: "prompt_text_file_contents_terminal_output_and_secrets_removed",
            }),
            "runtime_settings": to_value(RuntimeSettings {
                developer: RuntimeDeveloperSettings {
                    acp_trace: AcpTraceStatus {
                        enabled: true,
                        directory: "/workspace/app/.openaide/diagnostics/acp-traces".to_string(),
                    },
                },
            }),
        },
        "notifications": {
            "task_updated": to_value(RuntimeNotification {
                jsonrpc: "2.0",
                method: "task.updated",
                params: json!({ "task_id": "task_1", "revision": 9 }),
            }),
        },
        "host": {
            "request": to_value(HostRequest {
                jsonrpc: "2.0",
                id: RpcId::String("host_1".to_string()),
                method: "fs/read_text_file".to_string(),
                params: Some(json!({ "path": "/workspace/app/README.md" })),
            }),
        },
    });

    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}

fn task_summary() -> TaskSummary {
    TaskSummary {
        task_id: "task_1".to_string(),
        title: "Update docs".to_string(),
        status: TaskStatus::Blocked,
        task_version: 4,
        message_history_version: 6,
        unread: true,
        created_at: "2026-05-22T00:00:00Z".to_string(),
        updated_at: "2026-05-22T00:00:05Z".to_string(),
        last_activity: "2026-05-22T00:00:05Z".to_string(),
        agent_id: "codex".to_string(),
        agent_name: "Codex".to_string(),
        isolation: IsolationKind::Local,
        workspace_root: "/workspace/app".to_string(),
    }
}

fn attachment() -> Attachment {
    Attachment {
        kind: "file".to_string(),
        label: "README.md".to_string(),
        path: Some("/workspace/app/README.md".to_string()),
        payload: Some(json!({ "mime": "text/markdown" })),
    }
}

fn chat_message(message: NormalizedMessage) -> ChatMessage {
    ChatMessage {
        cursor: format!("cursor_{}", message.identity()),
        identity: message.identity(),
        message_type: message.message_type().to_string(),
        message_id: format!("row_{}", message.identity()),
        message,
    }
}

fn tool_details() -> ActivityToolDetails {
    ActivityToolDetails {
        locations: vec![ActivityToolLocation {
            path: "README.md".to_string(),
            line: Some(12),
        }],
        content: vec![
            ActivityToolContent::Text {
                text: "read output".to_string(),
            },
            ActivityToolContent::Diff {
                path: "README.md".to_string(),
                old_text: Some("old".to_string()),
                new_text: "new".to_string(),
            },
            ActivityToolContent::Terminal {
                terminal_id: "terminal_1".to_string(),
            },
            ActivityToolContent::Other {
                label: "extra".to_string(),
            },
        ],
        input: Some(ActivityToolInput {
            command: vec![
                "bash".to_string(),
                "-lc".to_string(),
                "cat README.md".to_string(),
            ],
            cwd: Some("/workspace/app".to_string()),
            query: Some("OpenAIDE".to_string()),
            queries: Vec::new(),
            url: Some("https://example.test".to_string()),
            path: Some("/workspace/app/README.md".to_string()),
            fields: vec![ActivityToolField {
                name: "mode".to_string(),
                value: "read".to_string(),
            }],
        }),
        output: Some(ActivityToolOutput {
            stdout: Some("ok".to_string()),
            stderr: Some("".to_string()),
            formatted_output: Some("formatted".to_string()),
            aggregated_output: Some("aggregate".to_string()),
            exit_code: Some(0),
            success: Some(true),
            fields: vec![ActivityToolField {
                name: "bytes".to_string(),
                value: "128".to_string(),
            }],
        }),
    }
}

fn to_value(value: impl serde::Serialize) -> Value {
    serde_json::to_value(value).unwrap()
}
