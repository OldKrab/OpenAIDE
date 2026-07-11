import type { AgentCatalogEntry, RuntimeAgentCatalogRecord } from "./types.js";

export function runtimeAgentCatalog(records: readonly AgentCatalogEntry[]): RuntimeAgentCatalogRecord[] {
  return records
    .filter((agent) => agent.enabled)
    .map(({ id, label, description, source_kind, enabled, transport, command, args, env, secret_env }) => ({
      id,
      label,
      description,
      source_kind,
      enabled,
      transport,
      command,
      args,
      env,
      secret_env,
    }));
}

