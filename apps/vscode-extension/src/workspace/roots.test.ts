import { beforeEach, describe, expect, it, vi } from "vitest";
import * as vscode from "vscode";
import { currentWorkspaceRoot, workspaceRoots } from "./roots";

const mocks = vi.hoisted(() => ({ activeUri: undefined as { fsPath: string } | undefined, folders: [] as Array<{ name: string; uri: { fsPath: string } }> }));

vi.mock("vscode", () => ({
  window: { get activeTextEditor() { return mocks.activeUri ? { document: { uri: mocks.activeUri } } : undefined; } },
  workspace: {
    get workspaceFolders() { return mocks.folders; },
    getWorkspaceFolder: (uri: { fsPath: string }) => mocks.folders.find((folder) => uri.fsPath.startsWith(folder.uri.fsPath)),
  },
}));

describe("VS Code workspace context", () => {
  beforeEach(() => {
    mocks.activeUri = undefined;
    mocks.folders = [
      { name: "API", uri: { fsPath: "/workspace/api" } },
      { name: "Web", uri: { fsPath: "/workspace/web" } },
    ];
  });

  it("uses the active editor workspace and supplies its deterministic Project Context", () => {
    mocks.activeUri = { fsPath: "/workspace/web/src/app.ts" };

    expect(currentWorkspaceRoot()).toEqual({
      label: "Web",
      path: "/workspace/web",
      projectId: "project-6faa96a0ab25b436",
    });
    expect(workspaceRoots()[0]?.label).toBe("Web");
  });

  it("falls back to the first workspace and returns no context without an open folder", () => {
    expect(currentWorkspaceRoot()?.label).toBe("API");
    mocks.folders = [];
    expect(currentWorkspaceRoot()).toBeUndefined();
  });
});
