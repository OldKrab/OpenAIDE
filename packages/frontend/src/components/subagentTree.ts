import type { SubagentCatalogEntrySnapshot } from "@openaide/app-server-client";

/** One piece of the line art tying a row to its parent and to the siblings below it. */
export type SubagentTreeGuide = { level: number; part: "elbow" | "rail" | "stem" };

export type SubagentTreeRow = {
  depth: number;
  entry: SubagentCatalogEntrySnapshot;
  guides: SubagentTreeGuide[];
};

/**
 * Lays the spawn-ordered catalog out as a tree. Spawn order alone can put a child under an unrelated
 * sibling, so each child moves directly below its parent; siblings keep their spawn order. An entry
 * whose parent is unknown, or that sits in a parent cycle, starts its own branch rather than vanish.
 */
export function subagentTreeRows(entries: SubagentCatalogEntrySnapshot[]): SubagentTreeRow[] {
  const known = new Set(entries.map((entry) => entry.subagentId));
  const children = new Map<string, SubagentCatalogEntrySnapshot[]>();
  for (const entry of entries) {
    if (!entry.parentSubagentId || !known.has(entry.parentSubagentId)) continue;
    children.set(entry.parentSubagentId, [...(children.get(entry.parentSubagentId) ?? []), entry]);
  }
  const rows: SubagentTreeRow[] = [];
  const visited = new Set<string>();
  /** `rails[level]` says a later sibling at that level still needs its line drawn past this row. */
  const walk = (entry: SubagentCatalogEntrySnapshot, rails: boolean[]) => {
    visited.add(entry.subagentId);
    const depth = rails.length;
    const kids = (children.get(entry.subagentId) ?? []).filter((kid) => !visited.has(kid.subagentId));
    const guides: SubagentTreeGuide[] = rails.flatMap((on, level) => (on ? [{ level, part: "rail" as const }] : []));
    if (depth > 0) guides.push({ level: depth - 1, part: "elbow" });
    if (kids.length > 0) guides.push({ level: depth, part: "stem" });
    rows.push({ depth, entry, guides });
    kids.forEach((kid, index) => {
      if (!visited.has(kid.subagentId)) walk(kid, [...rails, index < kids.length - 1]);
    });
  };
  for (const entry of entries) {
    if (!entry.parentSubagentId || !known.has(entry.parentSubagentId)) walk(entry, []);
  }
  for (const entry of entries) {
    if (!visited.has(entry.subagentId)) walk(entry, []);
  }
  return rows;
}
