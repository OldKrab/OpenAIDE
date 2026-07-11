import type { AgentIconId } from "./icons.js";

export type AgentCatalogEntry = {
  id: string;
  label: string;
  description: string;
  source_kind: "built_in" | "custom";
  icon: AgentIconId;
  enabled: boolean;
  transport: "stdio";
  command_line: string;
  command: string;
  args: string[];
  env: Record<string, string>;
  secret_env: string[];
};

export type CustomAgentSettingsRecord = {
  id?: unknown;
  label?: unknown;
  description?: unknown;
  icon?: unknown;
  enabled?: unknown;
  command_line?: unknown;
  command?: unknown;
  args?: unknown;
  env?: unknown;
  secret_env?: unknown;
};

export type RuntimeAgentCatalogRecord = Pick<
  AgentCatalogEntry,
  "id" | "label" | "description" | "source_kind" | "enabled" | "transport" | "command" | "args" | "env" | "secret_env"
>;

