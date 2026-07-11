import { agentCatalogEntry } from "./builtins.js";

export function agentDisplayLabel(agentId: string, selectedLabel?: string | null): string {
  const label = selectedLabel?.trim();
  if (label) return label.slice(0, 80);
  return agentCatalogEntry(agentId)?.label ?? agentId;
}

