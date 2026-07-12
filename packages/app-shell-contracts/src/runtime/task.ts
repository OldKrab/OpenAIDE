import type { MessagePage, NormalizedMessage } from "./chat.js";
import type { AgentCommandsCatalog, ConfigOptionsCatalog } from "./agent.js";
import type { IsolationKind, TaskStatus } from "./primitives.js";

export type TaskSummary = {
  task_id: string;
  project_id?: string;
  project_label?: string;
  title: string;
  status: TaskStatus;
  task_version: number;
  message_history_version: number;
  has_messages: boolean;
  unread: boolean;
  created_at: string;
  updated_at: string;
  last_activity: string;
  agent_id: string;
  agent_name: string;
  isolation: IsolationKind;
  workspace_root: string;
};

export type TaskSnapshot = {
  task: TaskSummary;
  chat: MessagePage;
  permissions: NormalizedMessage[];
  settings_summary: {
    agent_id: string;
    isolation: IsolationKind;
    model_id?: string;
    config_options?: Record<string, string>;
  };
  agent_config?: ConfigOptionsCatalog;
  agent_commands?: AgentCommandsCatalog;
  send_capability: {
    state: "loading" | "ready" | "blocked" | "failed";
    attachment_only: boolean;
    blockers?: Array<{
      kind:
        | "taskPreparing"
        | "taskRunning"
        | "agentConfigNotReady"
        | "slashCommandsNotReady"
        | "attachmentsNeedRefresh"
        | "emptyMessage"
        | "missingRequiredOptions"
        | "failedValidation";
      message: string;
    }>;
  };
  revision: number;
  history_sync: HistorySyncState;
};

export type HistorySyncState =
  | { state: "idle"; generation: number }
  | { state: "checking"; generation: number }
  | { state: "syncing"; generation: number }
  | { state: "updated"; generation: number }
  | { state: "failed"; generation: number; message: string; before_send: boolean };

export type TaskListResult = {
  tasks: TaskSummary[];
  revision: number;
  archived: boolean;
};
