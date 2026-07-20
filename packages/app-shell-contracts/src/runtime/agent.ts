import type { AgentIconId } from "../agentCatalog.js";
import type {
  ConfigOptionCategory,
  ConfigOptionsStatus,
} from "./primitives.js";

export type CustomAgentEnvRecord = {
  name: string;
  value?: string;
  secret: boolean;
};

export type CustomAgentCreateParams = {
  label: string;
  icon: AgentIconId;
  command_line: string;
  enabled: boolean;
  env: CustomAgentEnvRecord[];
};

export type CustomAgentMetadataUpdateParams = {
  agent_id: string;
  label: string;
  icon: AgentIconId;
  enabled: boolean;
};

export type CustomAgentReplaceParams = CustomAgentCreateParams & {
  source_agent_id: string;
  confirmed: boolean;
};

export type ConfigOptionsCatalog = {
  agent_id: string;
  status: ConfigOptionsStatus;
  options: ConfigOption[];
  pending_change?: {
    mutation_id: string;
    option_id: string;
    requested_value: ConfigOptionCurrentValue;
  };
  error?: string;
};

export type ConfigOption = {
  id: string;
  label: string;
  description?: string;
  category?: ConfigOptionCategory;
  kind: "select" | "boolean";
  current_value: ConfigOptionCurrentValue;
  values: ConfigOptionValue[];
};

export type ConfigOptionCurrentValue =
  | { type: "id"; value: string }
  | { type: "boolean"; value: boolean };

export type ConfigOptionValue = {
  id: string;
  label: string;
  description?: string;
  group_id?: string;
  group_label?: string;
};

export type AgentCommandsCatalog = {
  agent_id: string;
  status: ConfigOptionsStatus;
  commands: AgentSlashCommand[];
};

export type AgentSlashCommand = {
  name: string;
  description: string;
  input_hint?: string;
};

export type AgentAuthMethodSummary = {
  id: string;
  label: string;
  kind: string;
  description?: string;
};

export type AgentListedSession = {
  session_id: string;
  cwd: string;
  title?: string;
  last_activity?: string;
  updated_at?: string;
};

export type AgentListSessionsResult = {
  agent_id: string;
  sessions: AgentListedSession[];
  next_cursor?: string;
};
