import { describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  registerWebviewViewProvider: vi.fn(() => ({ dispose: vi.fn() })),
  registerCommands: vi.fn(),
  workspaceReady: new Promise<void>(() => undefined),
  notificationRegistration: new Promise<{ dispose(): void }>(() => undefined),
}));

vi.mock("vscode", () => ({
  window: {
    registerWebviewViewProvider: mocks.registerWebviewViewProvider,
  },
}));
vi.mock("./commands", () => ({ registerCommands: mocks.registerCommands }));
vi.mock("./logging/logger", () => ({ ExtensionLogger: class {} }));
vi.mock("./runtime/process", () => ({ RuntimeProcess: class { dispose() {} } }));
vi.mock("./runtime/rpcClient", () => ({ RuntimeClient: class { dispose() {} } }));
vi.mock("./runtime/hostFileSystem", () => ({
  registerFileSystemHostHandlers: () => ({ dispose() {} }),
}));
vi.mock("./runtime/hostAgentSecrets", () => ({
  registerAgentSecretHandlers: () => ({ dispose() {} }),
}));
vi.mock("./runtime/hostAgentAuthTerminal", () => ({
  registerAgentAuthTerminalHandler: () => ({ dispose() {} }),
}));
vi.mock("./runtime/hostTerminal", () => ({
  registerTerminalHostHandlers: () => ({ dispose() {} }),
}));
vi.mock("./workspace/projectSync", () => ({
  registerWorkspaceProjectSync: () => ({ ready: mocks.workspaceReady, dispose() {} }),
}));
vi.mock("./notifications/taskNotifications", () => ({
  registerTaskNotifications: () => mocks.notificationRegistration,
}));
vi.mock("./webview/editorManager", () => ({
  TaskEditorManager: class {
    updateWorkspaceRoots() {}
    dispose() {}
  },
}));
vi.mock("./webview/navigationProvider", () => ({
  TaskViewProvider: class {
    static viewType = "openaide.tasks";
    updateWorkspaceRoots() {}
    dispose() {}
  },
}));

import { activate } from "./extension";

describe("VS Code extension activation", () => {
  it("registers the Tasks view without waiting for App Server startup", async () => {
    const context = {
      subscriptions: [] as Array<{ dispose(): unknown }>,
      globalState: {},
      secrets: {},
    };

    const activation = activate(context as never);
    let settled = false;
    void activation.then(() => {
      settled = true;
    });
    await Promise.resolve();
    await Promise.resolve();

    expect(mocks.registerWebviewViewProvider).toHaveBeenCalledOnce();
    expect(mocks.registerCommands).toHaveBeenCalledOnce();
    expect(settled).toBe(true);
  });
});
