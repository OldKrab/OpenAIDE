import { builtInAgents } from "./builtins.js";
import { normalizedAgentIcon } from "./icons.js";
import type { AgentCatalogEntry, CustomAgentSettingsRecord } from "./types.js";

export function customAgentsFromSettings(value: unknown): AgentCatalogEntry[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((item) => {
    if (!item || typeof item !== "object") return [];
    const record = item as CustomAgentSettingsRecord;
    const id = normalizedIdentifier(record.id);
    const command = normalizedString(record.command);
    const commandLine = normalizedString(record.command_line);
    if (!id || !command || !commandLine) return [];
    return [
      {
        id,
        label: normalizedString(record.label) ?? id,
        description: normalizedString(record.description) ?? "Custom ACP stdio Agent.",
        source_kind: "custom",
        icon: normalizedAgentIcon(record.icon) ?? "bot",
        enabled: record.enabled === false ? false : true,
        transport: "stdio",
        command_line: commandLine,
        command,
        args: stringArray(record.args),
        env: stringRecord(record.env),
        secret_env: stringArray(record.secret_env).filter(isEnvironmentName),
      } satisfies AgentCatalogEntry,
    ];
  });
}

export function agentCatalogFromSettings(customAgents: unknown): AgentCatalogEntry[] {
  const raw = Array.isArray(customAgents) ? customAgents : [];
  const builtInOverrides = new Map(
    raw.flatMap((item) => {
      if (!item || typeof item !== "object") return [];
      const record = item as CustomAgentSettingsRecord;
      const id = normalizedIdentifier(record.id);
      return id ? [[id, record.enabled !== false] as const] : [];
    }),
  );
  const custom = customAgentsFromSettings(customAgents);
  const builtInIds = new Set<string>(builtInAgents.map((agent) => agent.id));
  return [
    ...builtInAgents.map((agent) => ({ ...agent, enabled: builtInOverrides.get(agent.id) ?? agent.enabled })),
    ...custom.filter((agent) => !builtInIds.has(agent.id)),
  ];
}

function normalizedIdentifier(value: unknown): string | undefined {
  const text = normalizedString(value);
  return text && /^[a-zA-Z0-9_.-]+$/.test(text) ? text : undefined;
}

function normalizedString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : undefined;
}

function stringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : [];
}

function stringRecord(value: unknown): Record<string, string> {
  if (!value || typeof value !== "object" || Array.isArray(value)) return {};
  return Object.fromEntries(
    Object.entries(value).filter((entry): entry is [string, string] => {
      const [name, fieldValue] = entry;
      return isEnvironmentName(name) && typeof fieldValue === "string";
    }),
  );
}

function isEnvironmentName(value: string): boolean {
  return /^[A-Za-z_][A-Za-z0-9_]*$/.test(value);
}

