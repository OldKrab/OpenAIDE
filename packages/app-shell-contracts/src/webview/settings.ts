import type { AgentIconId } from "../agentCatalog.js";
import type { CustomAgentEnvRecord } from "../runtime/agent.js";
import type { RuntimeDiagnostics } from "../runtime/system.js";
import type { SettingsScope } from "./preferences.js";

export type DiagnosticsSnapshot = {
  created_at: string;
  runtime: RuntimeDiagnostics;
  notices: Array<{
    component: "runtime" | "host";
    severity: "info" | "warning" | "error";
    message: string;
  }>;
  process: {
    running: boolean;
    runtime_source_kind: "configured" | "environment" | "bundled" | "development";
    storage_root_kind: "configured" | "environment" | "extension-storage";
  };
};

export type WorkspaceRootSummary = {
  path: string;
  label: string;
  projectId?: string;
};

export type AgentSignInPhase = "starting" | "awaiting_user" | "awaiting_terminal" | "failed";

export type AgentSignInFlowRecord = {
  method_id: string;
  phase: AgentSignInPhase;
  /** HTTPS verification page the user must open while `awaiting_user`. */
  url?: string;
  /** Agent-supplied instructions shown next to the URL, such as a one-time device code. */
  hint?: string;
  /** Product-safe failure summary while `failed`. */
  failure?: string;
};

export type AgentSettingsRecord = {
  id: string;
  label: string;
  enabled: boolean;
  scope: SettingsScope;
  source_kind: "built_in" | "custom";
  icon: AgentIconId;
  transport: "stdio";
  status: "unprobed" | "ready" | "failed" | "disabled" | "setup_required" | "auth_required" | "authenticating" | "unsupported" | "installing" | "launching" | "connected" | "disconnected";
  setup_reason?: "nodeJsRequired";
  launch_label: string;
  command_line?: string;
  env?: CustomAgentEnvRecord[];
  description: string;
  capabilities: string[];
  protocol_version?: string;
  implementation_version?: string;
  auth_methods: Array<{
    id: string;
    label: string;
    kind: string;
    description?: string;
    variables?: Array<{ name: string; label?: string; secret: boolean; optional: boolean }>;
    link?: string;
    terminal_args?: string[];
    terminal_env?: Record<string, string>;
  }>;
  logout_supported?: boolean;
  logout_blocked_by_running_task?: boolean;
  /** Cleanup provenance only; it does not assert that the Agent is currently signed in. */
  last_authentication_method_id?: string;
  /**
   * App Server-owned Sign-in Flow for this Agent. Present while a flow runs or after it failed;
   * absent after success or cancellation. Frontend renders it and never derives its own copy.
   */
  sign_in?: AgentSignInFlowRecord;
  last_checked_at?: string;
  last_error_summary?: string;
};

export type McpServerSettingsRecord = {
  id: string;
  label: string;
  enabled: boolean;
  scope: { kind: "global" } | { kind: "project"; projectId: string };
  transport: "stdio" | "http" | "sse";
  status: "configured" | "invalid" | "disabled";
  description?: string;
  validation_error?: string;
};

export type SettingsProjectionAvailability = "available" | "unavailable";

export type SkillSettingsRecord = {
  id: string;
  label: string;
  scope: SettingsScope;
  source_label: string;
  status: "valid" | "warning" | "invalid" | "shadowed";
  description?: string;
  warnings: string[];
  tags: string[];
  last_scanned_at: string;
};

export type SkillSettingsDetails = {
  generated_at: string;
  skill: SkillSettingsRecord;
  document: {
    name: string;
    description: string;
    additional_fields: Array<{
      name: string;
      value: string;
    }>;
    instructions: string;
    source: string;
  };
};
