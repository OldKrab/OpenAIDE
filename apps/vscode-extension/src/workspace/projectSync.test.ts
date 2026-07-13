import { beforeEach, describe, expect, it, vi } from "vitest";
import { registerWorkspaceProjectSync } from "./projectSync";

const mocks = vi.hoisted(() => ({
  roots: [] as Array<{ path: string; label: string; projectId: string }>,
  listener: undefined as (() => void) | undefined,
  dispose: vi.fn(),
}));

vi.mock("vscode", () => ({
  workspace: {
    onDidChangeWorkspaceFolders(listener: () => void) {
      mocks.listener = listener;
      return { dispose: mocks.dispose };
    },
  },
}));

vi.mock("./roots", () => ({
  workspaceRoots: () => mocks.roots,
}));

describe("VS Code workspace Project synchronization", () => {
  beforeEach(() => {
    mocks.roots = [{
      path: "/workspace/app",
      label: "App",
      projectId: "project-app",
    }];
    mocks.listener = undefined;
    mocks.dispose.mockClear();
  });

  it("reports the initial workspace roots before webviews start", async () => {
    const runtime = { syncWorkspaceRoots: vi.fn(async () => undefined) };
    const logger = { warn: vi.fn() };

    const sync = registerWorkspaceProjectSync(runtime, logger);
    await sync.ready;

    expect(runtime.syncWorkspaceRoots).toHaveBeenCalledWith([{ path: "/workspace/app" }]);
    expect(logger.warn).not.toHaveBeenCalled();
  });

  it("reports the complete replacement set when workspace folders change", async () => {
    const runtime = { syncWorkspaceRoots: vi.fn(async () => undefined) };
    const sync = registerWorkspaceProjectSync(runtime, { warn: vi.fn() });
    await sync.ready;
    mocks.roots = [
      { path: "/workspace/api", label: "API", projectId: "project-api" },
      { path: "/workspace/web", label: "Web", projectId: "project-web" },
    ];

    mocks.listener?.();

    await vi.waitFor(() => {
      expect(runtime.syncWorkspaceRoots).toHaveBeenLastCalledWith([
        { path: "/workspace/api" },
        { path: "/workspace/web" },
      ]);
    });
    sync.dispose();
    expect(mocks.dispose).toHaveBeenCalledOnce();
  });

  it("keeps activation recoverable when the initial App Server sync fails", async () => {
    const runtime = {
      syncWorkspaceRoots: vi.fn(async () => {
        throw new Error("unavailable");
      }),
    };
    const logger = { warn: vi.fn() };

    const sync = registerWorkspaceProjectSync(runtime, logger);
    await expect(sync.ready).resolves.toBeUndefined();

    expect(logger.warn).toHaveBeenCalledWith(
      "failed to synchronize VS Code workspace Projects",
      { error: "Error: unavailable" },
    );
  });
});
