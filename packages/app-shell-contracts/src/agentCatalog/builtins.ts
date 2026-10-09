import type { AgentCatalogEntry } from "./types.js";
import claudeAcpPolicy from "./claude-acp/package.json" with { type: "json" };

// Shared with App Server: update-claude-acp.yml updates this exact launch pin in one place.
const claudeAcpSpec = `@openaide/claude-agent-acp@${claudeAcpPolicy.dependencies["@openaide/claude-agent-acp"]}`;

export const builtInAgents = [
  {
    id: "codex",
    label: "Codex",
    description: "OpenAI coding agent.",
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
    description: "Open-source coding agent.",
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
  {
    id: "claude-code",
    label: "Claude Code",
    description: "Anthropic coding agent.",
    source_kind: "built_in",
    icon: "claude",
    enabled: true,
    transport: "stdio",
    command_line: `npx -y ${claudeAcpSpec}`,
    command: "npx",
    args: ["-y", claudeAcpSpec],
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
