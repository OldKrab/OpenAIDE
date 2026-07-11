import { describe, expect, it } from "vitest";
import {
  agentProjectRequestKey,
  configOptionsRequestKey,
  shouldLoadNativeSessions,
  shouldLoadNewTaskConfigOptions,
  shouldRequestWorkspaceRoots,
} from "./surfaceRouting";

describe("webview surface routing", () => {
  it("loads agent options only for an unsent task editor surface", () => {
    expect(shouldLoadNewTaskConfigOptions({ surface: "task" }, false, "project_1")).toBe(true);
    expect(shouldLoadNewTaskConfigOptions({ surface: "task", taskId: "task_1" }, false, "project_1")).toBe(false);
    expect(shouldLoadNewTaskConfigOptions({ surface: "task" }, true, "project_1")).toBe(false);
    expect(shouldLoadNewTaskConfigOptions({ surface: "navigation" }, false, "project_1")).toBe(false);
    expect(shouldLoadNewTaskConfigOptions({ surface: "settings" }, false, "project_1")).toBe(false);
    expect(shouldLoadNewTaskConfigOptions({ surface: "invalid" }, false, "project_1")).toBe(false);
  });

  it("waits for a concrete project before preparing agent options", () => {
    expect(shouldLoadNewTaskConfigOptions({ surface: "task" }, false, undefined)).toBe(false);
    expect(shouldLoadNewTaskConfigOptions({ surface: "task" }, false, "")).toBe(false);
    expect(shouldLoadNewTaskConfigOptions({ surface: "task" }, false, "   ")).toBe(false);
    expect(shouldLoadNewTaskConfigOptions({ surface: "task" }, false, "project_1")).toBe(true);
  });

  it("does not request workspace roots for invalid surfaces", () => {
    expect(shouldRequestWorkspaceRoots({ surface: "task" })).toBe(true);
    expect(shouldRequestWorkspaceRoots({ surface: "navigation" })).toBe(true);
    expect(shouldRequestWorkspaceRoots({ surface: "settings" })).toBe(true);
    expect(shouldRequestWorkspaceRoots({ surface: "invalid" })).toBe(false);
  });

  it("loads native sessions for usable surfaces when a project is available", () => {
    expect(shouldLoadNativeSessions({ surface: "navigation" }, "project_1")).toBe(true);
    expect(shouldLoadNativeSessions({ surface: "task" }, "project_1")).toBe(true);
    expect(shouldLoadNativeSessions({ surface: "settings" }, "project_1")).toBe(true);
    expect(shouldLoadNativeSessions({ surface: "task" }, undefined)).toBe(false);
    expect(shouldLoadNativeSessions({ surface: "task" }, "")).toBe(false);
    expect(shouldLoadNativeSessions({ surface: "invalid" }, "project_1")).toBe(false);
  });

  it("keys prepared options by agent and project", () => {
    expect(configOptionsRequestKey("codex", "project_1")).toBe("codex\u0000project_1");
    expect(configOptionsRequestKey("codex", "")).toBe("codex\u0000");
    expect(agentProjectRequestKey("codex", "project_1")).toBe("codex\u0000project_1");
  });
});
