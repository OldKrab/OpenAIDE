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

export type AgentSettingsRecord = {
  id: string;
  label: string;
  enabled: boolean;
  scope: SettingsScope;
  source_kind: "built_in" | "custom";
  icon: AgentIconId;
  transport: "stdio";
  status: "unprobed" | "ready" | "failed" | "disabled" | "setup_required" | "auth_required" | "authenticating" | "unsupported" | "launching" | "connected" | "disconnected";
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
  authenticating_method_id?: string;
  last_checked_at?: string;
  last_error_summary?: string;
};

export type McpServerSettingsRecord = {
  id: string;
  label: string;
  enabled: boolean;
  scope: SettingsScope;
  transport: "stdio" | "http" | "sse";
  status: "unknown" | "available" | "failed" | "disabled";
  description?: string;
  tool_count?: number;
  last_checked_at?: string;
  last_error_summary?: string;
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
