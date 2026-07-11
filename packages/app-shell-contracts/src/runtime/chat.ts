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
  | { kind: "agent_text"; id: string; text: string; created_at: string; streaming?: boolean }
  | { kind: "thought"; id: string; text: string; created_at: string; streaming?: boolean }
  | { kind: "activity"; id: string; title: string; status: ActivityStatus; created_at: string; collapsed: boolean; steps: ActivityStep[] }
  | { kind: "permission"; id: string; request_id: string; app_server_request_id?: string; title: string; description?: string; scope?: string; risk?: string; tool_call: PermissionToolCall; state: PermissionState; created_at: string; options: PermissionOption[]; selected_option?: string; decision?: PermissionDecision }
  | ElicitationMessage
  | { kind: "interruption"; id: string; reason: InterruptionReason; message: string; created_at: string; recoverable: boolean };

export type Attachment = {
  /** Stable identity from the App Server projection; labels are not unique. */
  id?: string;
  kind: "file" | "context" | "text";
  label: string;
  path?: string;
  payload?: unknown;
};

export type ActivityStep =
  | { kind: "text"; text: string; level?: "info" | "warning" | "error" }
  | { kind: "thought"; text: string; streaming?: boolean }
  | { kind: "tool"; tool_call_id?: string; name: string; status: ActivityStatus; input_summary?: string; output_preview?: string; detail_artifact_id?: string; details?: ActivityToolDetails }
  | { kind: "command"; command_label: string; status: ActivityStatus; exit_code?: number; output_preview?: string };

export type ActivityToolDetails = {
  locations: ActivityToolLocation[];
  content: ActivityToolContent[];
  input?: ActivityToolInput;
  output?: ActivityToolOutput;
};

export type ActivityToolLocation = {
  path: string;
  line?: number;
};

export type ActivityToolContent =
  | { kind: "text"; text: string }
  | { kind: "diff"; path: string; old_text?: string; new_text: string }
  | { kind: "terminal"; terminal_id: string }
  | { kind: "other"; label: string };

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
  value: string;
};

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
