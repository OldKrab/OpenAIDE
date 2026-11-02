import type { SubagentCatalogEntrySnapshot } from "@openaide/app-server-client";
import { describe, expect, it } from "vitest";
import { subagentTreeRows } from "./subagentTree";

const entry = (subagentId: string, parentSubagentId?: string) => ({
  capabilities: { cancel: false, close: false },
  delegatedTask: subagentId,
  historyRevision: 0,
  name: subagentId,
  parentSubagentId,
  spawnedOrder: 0,
  status: "running",
  subagentId,
}) as unknown as SubagentCatalogEntrySnapshot;

const shape = (entries: SubagentCatalogEntrySnapshot[]) => subagentTreeRows(entries).map(({ depth, entry: { subagentId }, guides }) => (
  `${subagentId}:${depth}:${guides.map(({ level, part }) => `${part}${level}`).join(",")}`
));

describe("subagentTreeRows", () => {
  it("keeps flat Subagents flat and in spawn order", () => {
    expect(shape([entry("a"), entry("b")])).toEqual(["a:0:", "b:0:"]);
  });

  it("moves a child below its parent when spawn order put a sibling between them", () => {
    expect(shape([entry("a"), entry("b"), entry("a1", "a")])).toEqual(["a:0:stem0", "a1:1:elbow0", "b:0:"]);
  });

  it("carries a rail past a nested branch to the next sibling, and stops at the last one", () => {
    expect(shape([entry("a"), entry("a1", "a"), entry("a2", "a"), entry("a1x", "a1"), entry("b")])).toEqual([
      "a:0:stem0",
      "a1:1:rail0,elbow0,stem1",
      "a1x:2:rail0,elbow1",
      "a2:1:elbow0",
      "b:0:",
    ]);
  });

  it("keeps a Subagent whose parent is not in the catalog", () => {
    expect(shape([entry("orphan", "missing")])).toEqual(["orphan:0:"]);
  });

  it("keeps every Subagent of a parent cycle exactly once", () => {
    expect(shape([entry("a", "b"), entry("b", "a")])).toEqual(["a:0:stem0", "b:1:elbow0"]);
  });
});
