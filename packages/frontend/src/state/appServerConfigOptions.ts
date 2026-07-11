import type { ConfigOptionsCatalog } from "@openaide/app-shell-contracts";
import type { AgentConfigOptionsResult } from "@openaide/app-server-client";

export function mapProtocolConfigOptions(result: AgentConfigOptionsResult): ConfigOptionsCatalog {
  return {
    agent_id: result.catalog.agentId,
    status: result.catalog.status,
    options: result.catalog.options.map((option) => ({
      id: option.id,
      label: option.label,
      description: option.description ?? undefined,
      category: option.category ?? undefined,
      current_value: option.currentValue,
      values: option.values.map((value) => ({
        id: value.id,
        label: value.label,
        description: value.description ?? undefined,
        group_id: value.groupId ?? undefined,
        group_label: value.groupLabel ?? undefined,
      })),
    })),
  };
}
