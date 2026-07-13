import type { ConfigOptionsCatalog } from "@openaide/app-shell-contracts";

/** Ready-empty is a valid Agent catalog, not a loading sentinel. */
export function configOptionsSettled(catalog: ConfigOptionsCatalog | undefined) {
  return catalog?.status === "ready" || catalog?.status === "empty";
}

export function configOptionsMutable(catalog: ConfigOptionsCatalog | undefined) {
  return configOptionsSettled(catalog) && catalog?.pending_change === undefined;
}
