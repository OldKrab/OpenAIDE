import type { ConfigOptionsCatalog } from "@openaide/app-shell-contracts";

/** Ready-empty is a valid Agent catalog, not a loading sentinel. */
export function configOptionsSettled(catalog: ConfigOptionsCatalog | undefined) {
  return catalog?.status === "ready" || catalog?.status === "empty";
}

export function configOptionsMutable(catalog: ConfigOptionsCatalog | undefined) {
  return configOptionsSettled(catalog) && catalog?.pending_change === undefined;
}

/** Identifies the complete Agent-owned catalog while excluding transient mutation presentation. */
export function configOptionsCatalogKey(catalog: ConfigOptionsCatalog | undefined) {
  if (!catalog) return undefined;
  return JSON.stringify({
    agent_id: catalog.agent_id,
    status: catalog.status,
    options: catalog.options,
  });
}
