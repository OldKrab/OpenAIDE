import type {
  ActivityStatus,
  InterruptionReason,
  PermissionDecision,
  PermissionOptionKind,
  PermissionState,
} from "./primitives.js";
import type { ElicitationMessage } from "./elicitation.js";

export type ChatMessage = {
  cursor: string;
  identity: string;
  message_type: string;
  message_id: string;
  message: NormalizedMessage;
};

export type NormalizedMessage =
  | { kind: "user"; id: string; text: string; created_at: string; attachments?: Attachment[] }
  | { kind: "agent_message"; id: string; role: AgentMessageRole; parts: AgentMessagePart[]; created_at: string }
  | { kind: "activity"; id: string; title: string; status: ActivityStatus; created_at: string; collapsed: boolean; steps: ActivityStep[] }
  | { kind: "permission"; id: string; request_id: string; app_server_request_id?: string; title: string; description?: string; scope?: string; risk?: string; tool_call: PermissionToolCall; state: PermissionState; created_at: string; options: PermissionOption[]; selected_option?: string; decision?: PermissionDecision; resolution_message?: string }
  | ElicitationMessage
  | { kind: "interruption"; id: string; reason: InterruptionReason; message: string; created_at: string; recoverable: boolean };

export type Attachment = {
  /** Stable identity from the App Server projection; labels are not unique. */
  id?: string;
  kind: "file" | "context" | "text" | "image";
  label: string;
  path?: string;
  payload?: unknown;
};

export type AgentMessageRole = "agent" | "thought";

/** Ordered App Server-owned content for one logical ACP Agent or Thought message. */
export type AgentMessagePart =
  | { kind: "text"; text: string }
  | { kind: "image"; media_type: string; data_url: string; uri?: string }
  | { kind: "resource"; uri: string; name?: string; title?: string; description?: string; media_type?: string; size_bytes?: number; text?: string }
  | { kind: "unsupported"; content_type: string; media_type?: string; uri?: string };

export type ActivityStep =
  | { kind: "text"; text: string; level?: "info" | "warning" | "error" }
  | { kind: "thought"; message_id?: string; text: string; streaming?: boolean }
  | { kind: "tool"; tool_call_id?: string; name: string; status: ActivityStatus; presentation?: ToolPresentation; input_summary?: string; output_preview?: string; detail_artifact_id?: string; details?: ActivityToolDetails; permission_outcomes?: ToolPermissionOutcome[] }
  | { kind: "command"; command_label: string; status: ActivityStatus; exit_code?: number; output_preview?: string };

/** Semantic compact-row chrome; the Tool's actual `name` still owns detail routing. */
export type ToolPresentation = {
  kind: "skill" | "read" | "list" | "search";
  subjects: string[];
};

export type ActivityToolDetails = {
  locations: ActivityToolLocation[];
  content: ActivityToolContent[];
  input?: ActivityToolInput;
  output?: ActivityToolOutput;
  terminal_outputs?: Array<{ terminal_id: string; output: string }>;
};

/** Durable authorization history for a tool, independent of its execution status. */
export type ToolPermissionOutcome = {
  request_id: string;
  decision: "approved" | "rejected" | "cancelled";
  option_id?: string;
  option_label?: string;
  resolved_at: string;
};

export type ActivityToolLocation = {
  path: string;
  line?: number;
};

export type ActivityToolContent =
  | { kind: "text"; text: string }
  | { kind: "diff"; path: string; old_text?: string; new_text: string }
  | { kind: "terminal"; terminal_id: string }
  | { kind: "image"; media_type: string; data_url: string; uri?: string }
  | { kind: "audio"; media_type: string; data_url: string }
  | { kind: "resource"; uri: string; name?: string; title?: string; description?: string; media_type?: string; size_bytes?: number; text?: string }
  | { kind: "unsupported"; content_type: string; media_type?: string; uri?: string };

export type ActivityToolInput = {
  command: string[];
  cwd?: string;
  query?: string;
  queries?: string[];
  url?: string;
  path?: string;
  fields: ActivityToolField[];
};

export type ActivityToolOutput = {
  stdout?: string;
  stderr?: string;
  formatted_output?: string;
  aggregated_output?: string;
  exit_code?: number;
  success?: boolean;
  fields: ActivityToolField[];
};

export type ActivityToolField = {
  name: string;
  value: ActivityToolValue;
};

export type ActivityToolValue =
  | { kind: "null" }
  | { kind: "boolean"; value: boolean }
  | { kind: "number"; value: string }
  | { kind: "string"; value: string }
  | { kind: "array"; items: ActivityToolValue[] }
  | { kind: "object"; fields: ActivityToolField[] }
  | { kind: "redacted" };

export type PermissionOption = {
  id: string;
  label: string;
  kind?: PermissionOptionKind;
  description?: string;
};

export type PermissionToolCall = {
  id: string;
  title: string;
  kind?: string;
};

export type MessagePage = {
  task_id: string;
  items: ChatMessage[];
  has_before: boolean;
  has_messages: boolean;
  total_count: number;
  version: number;
  start_cursor?: string;
  end_cursor?: string;
};
