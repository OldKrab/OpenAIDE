import type { ChatMessage, MessagePage } from "./chat.js";
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
  attention?: TaskAttentionEvent;
  created_at: string;
  updated_at: string;
  last_activity: string;
  agent_id: string;
  agent_name: string;
  isolation: IsolationKind;
  workspace_root: string;
  worktree_id?: string;
  workspace_available?: boolean;
  worktree_name?: string;
  git_ref?: string;
};

export type TaskAttentionEvent = {
  event_id: string;
  reason: "finished" | "needsPermission" | "needsAnswer" | "stopped" | "failed";
  occurred_at: string;
};

export type TaskSnapshot = {
  lifecycle: "new" | "visible";
  task: TaskSummary;
  /** App Server-authored start of the active turn; absent when no turn is running. */
  active_turn_started_at?: string;
  chat: MessagePage;
  /** Active App Server requests render after durable Chat and never enter history. */
  active_requests: ChatMessage[];
  settings_summary: {
    agent_id: string;
    isolation: IsolationKind;
    model_id?: string;
  };
  agent_config?: ConfigOptionsCatalog;
  agent_commands?: AgentCommandsCatalog;
  preparation?:
    | { kind: "preparing" }
    | { kind: "ready" }
    | {
        kind: "blocked";
        blocker: {
          kind: "authRequired" | "setupRequired" | "nodeJsRequired" | "capabilityUnavailable" | "nativeSessionUnavailable";
        };
      }
    | { kind: "failed" };
  send_capability: {
    state: "loading" | "ready" | "blocked" | "failed";
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
  input_capabilities?: {
    image: boolean;
  };
  revision: number;
  history_sync: HistorySyncState;
};

export type HistorySyncState =
  | { state: "idle"; generation: number }
  | { state: "syncing"; generation: number }
  | { state: "updated"; generation: number };

export type TaskListResult = {
  tasks: TaskSummary[];
  revision: number;
  archived: boolean;
};
