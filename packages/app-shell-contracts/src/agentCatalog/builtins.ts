import type { AgentCatalogEntry } from "./types.js";

export const builtInAgents = [
  {
    id: "codex",
    label: "Codex",
    description: "Built-in ACP Agent. Configuration Options are discovered before task start.",
    source_kind: "built_in",
    icon: "openai",
    enabled: true,
    transport: "stdio",
    command_line: "codex-acp",
    command: "codex-acp",
    args: [],
    env: {},
    secret_env: [],
  },
  {
    id: "opencode",
    label: "OpenCode",
    description: "Built-in ACP Agent. Configuration Options are discovered before task start.",
    source_kind: "built_in",
    icon: "opencode",
    enabled: true,
    transport: "stdio",
    command_line: "opencode acp",
    command: "opencode",
    args: ["acp"],
    env: {},
    secret_env: [],
  },
] as const satisfies readonly AgentCatalogEntry[];

export const defaultAgent = builtInAgents[0];

export function agentCatalogEntry(agentId: string): AgentCatalogEntry | undefined {
  return builtInAgents.find((agent) => agent.id === agentId);
}

export function resolveAgentCatalogEntry(agentId: string): AgentCatalogEntry {
  return agentCatalogEntry(agentId) ?? defaultAgent;
}

