import type { AgentCatalogEntry } from "./types.js";
import claudeAcpRuntime from "./claude-acp/package.json" with { type: "json" };
import codexAcpRuntime from "./codex-acp/package.json" with { type: "json" };

// Shared with App Server, which embeds these adapter commits (ADR-0079). `npm run
// adapters:lock` writes each identity from its fork submodule; the spec matches the
// App Server declaration, with the fork commit as semver build metadata.
function embeddedAdapterSpec(runtime: { openaideAdapter: { name: string; version: string; commit: string } }): string {
  const adapter = runtime.openaideAdapter;
  return `${adapter.name}@${adapter.version}+${adapter.commit.slice(0, 12)}`;
}
const claudeAcpSpec = embeddedAdapterSpec(claudeAcpRuntime);
const codexAcpSpec = embeddedAdapterSpec(codexAcpRuntime);

export const builtInAgents = [
  {
    id: "codex",
    label: "Codex",
    description: "OpenAI coding agent.",
    source_kind: "built_in",
    icon: "openai",
    enabled: true,
    transport: "stdio",
    command_line: `node ${codexAcpSpec}`,
    command: "node",
    args: [codexAcpSpec],
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
    command_line: `node ${claudeAcpSpec}`,
    command: "node",
    args: [claudeAcpSpec],
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
