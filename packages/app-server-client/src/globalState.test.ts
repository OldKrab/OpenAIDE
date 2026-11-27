import { describe, expect, it } from "vitest";
import type { GlobalState, WorktreeRepositoryId } from "./generated/protocol.js";
import { applyGlobalStateChanges } from "./globalState.js";

const repo = (id: string, revision = 1) =>
  ({ repositoryId: id as WorktreeRepositoryId, revision, worktrees: [] }) as never;

function state(): GlobalState {
  return {
    projects: { projects: [] },
    worktrees: [repo("repo_1"), repo("repo_2")],
    agents: { agents: [] },
    settings: { sections: [] },
    newTaskDefaults: {},
    devices: { remoteAccess: "off", serverName: "workstation" },
    terminals: { terminals: [] },
  } as GlobalState;
}

describe("applyGlobalStateChanges", () => {
  it("replaces every named area in one step and keeps the rest", () => {
    const before = state();
    const after = applyGlobalStateChanges(before, {
      devices: { remoteAccess: "on", serverName: "workstation" },
      agents: { agents: [{ agentId: "codex", label: "Codex", status: "connected" }] as never },
    });

    expect(after.devices.remoteAccess).toBe("on");
    expect(after.agents.agents).toHaveLength(1);
    expect(after.projects).toBe(before.projects);
    expect(after.terminals).toBe(before.terminals);
    expect(before.devices.remoteAccess).toBe("off");
  });

  it("upserts and removes worktree repositories by id", () => {
    const after = applyGlobalStateChanges(state(), {
      worktrees: [repo("repo_2", 5), repo("repo_3")],
      removedWorktreeRepositories: ["repo_1" as WorktreeRepositoryId],
    });

    expect(after.worktrees.map((r) => [r.repositoryId, r.revision])).toEqual([
      ["repo_2", 5],
      ["repo_3", 1],
    ]);
  });
});
