import { describe, expect, it, vi } from "vitest";
import type { ClientInstanceId } from "@openaide/app-server-client";
import { getClientInstanceId, initializeParamsForBootstrap, taskNavigationScopeForBootstrap } from "./backendInitialization";

const VSCODE_SHELL = { kind: "vscodeExtension", navigationMode: "currentProject" } as const;
const WEB_SHELL = { kind: "web", navigationMode: "project" } as const;

describe("backend initialization", () => {
  it("builds initialize params from the current shell surface", () => {
    expect(initializeParamsForBootstrap(
      { surface: "navigation", shell: VSCODE_SHELL },
      "client-1" as ClientInstanceId,
    )).toMatchObject({
      clientInstanceId: "client-1",
      shell: { kind: "vscodeExtension" },
      requestedSurface: { kind: "home" },
      capabilities: {
        protocol: [
          "requestResponses",
          "stableClientRequestIds",
          "resync",
        ],
        shell: [
          "openExternal",
          "revealFile",
          "pickLocalFile",
          "openTerminal",
          "readSecret",
          "writeSecret",
          "showNotification",
        ],
      },
    });

    expect(initializeParamsForBootstrap(
      { surface: "navigation", shell: VSCODE_SHELL, projectId: "project-1" },
      "client-1" as ClientInstanceId,
    ).requestedSurface).toEqual({ kind: "project", projectId: "project-1" });

    expect(initializeParamsForBootstrap(
      { surface: "task", shell: VSCODE_SHELL, taskId: "task-1" },
      "client-1" as ClientInstanceId,
    ).requestedSurface).toEqual({ kind: "task", taskId: "task-1" });

    expect(initializeParamsForBootstrap(
      { surface: "task", shell: VSCODE_SHELL },
      "client-1" as ClientInstanceId,
    ).requestedSurface).toEqual({ kind: "newTask" });

    expect(initializeParamsForBootstrap(
      { surface: "task", shell: VSCODE_SHELL, projectId: "project-1" },
      "client-1" as ClientInstanceId,
    ).requestedSurface).toEqual({ kind: "newTask", projectId: "project-1" });

    expect(initializeParamsForBootstrap(
      { surface: "settings", shell: VSCODE_SHELL },
      "client-1" as ClientInstanceId,
    ).requestedSurface).toEqual({ kind: "settings" });
  });

  it("uses the identity injected for a VS Code webview instead of shared session storage", () => {
    const sharedStorage = memoryStorage();
    sharedStorage.setItem("openaide.clientInstanceId", "shared-origin-client");
    vi.stubGlobal("sessionStorage", sharedStorage);

    expect(initializeParamsForBootstrap({
      surface: "task",
      shell: VSCODE_SHELL,
      clientInstanceId: "task-panel-1" as ClientInstanceId,
    }).clientInstanceId).toBe("task-panel-1");
    expect(initializeParamsForBootstrap({
      surface: "task",
      shell: VSCODE_SHELL,
      clientInstanceId: "task-panel-2" as ClientInstanceId,
    }).clientInstanceId).toBe("task-panel-2");

    vi.unstubAllGlobals();
  });

  it("uses explicit Web identity even with a local HTTP transport", () => {
    const initialized = initializeParamsForBootstrap(
      {
        surface: "task",
        shell: WEB_SHELL,
        appServerConnection: {
          kind: "localHttp",
          endpointUrl: "http://127.0.0.1:43123",
          authToken: "test-token",
        },
      },
      "client-web" as ClientInstanceId,
    );

    expect(initialized.shell).toEqual({ kind: "web" });
    expect(initialized.capabilities?.shell ?? []).not.toContain("readSecret");
    expect(initialized.capabilities?.shell ?? []).not.toContain("writeSecret");
  });

  it("scopes only the VS Code task list to its fixed Project Context", () => {
    expect(taskNavigationScopeForBootstrap({
      surface: "navigation",
      shell: VSCODE_SHELL,
      projectId: "project-1",
      appServerConnection: { kind: "webProxy", endpointUrl: "/transport-does-not-control-scope" },
    })).toEqual({
      kind: "taskNavigation",
      projectId: "project-1",
    });
    expect(taskNavigationScopeForBootstrap({
      surface: "task",
      shell: WEB_SHELL,
      projectId: "project-1",
      appServerConnection: { kind: "webProxy", endpointUrl: "/probe" },
    })).toEqual({ kind: "taskNavigation" });
  });

  it("uses session storage for stable tab identity", () => {
    const storage = memoryStorage();
    vi.stubGlobal("crypto", { randomUUID: () => "client-2" });

    expect(getClientInstanceId(storage)).toBe("client-2");
    expect(getClientInstanceId(storage)).toBe("client-2");
    expect(storage.getItem("openaide.clientInstanceId")).toBe("client-2");

    vi.unstubAllGlobals();
  });

  it("gives a duplicated browser tab its own logical client identity", () => {
    const originalStorage = memoryStorage();
    const originalTab = { name: "", navigationType: "navigate" as const };
    const originalId = getClientInstanceId(originalStorage, originalTab);
    const duplicatedStorage = copyStorage(originalStorage);
    const duplicatedTab = { name: originalTab.name, navigationType: "navigate" as const };

    const duplicatedId = getClientInstanceId(duplicatedStorage, duplicatedTab);

    expect(duplicatedId).not.toBe(originalId);
    expect(getClientInstanceId(duplicatedStorage, {
      name: duplicatedTab.name,
      navigationType: "reload",
    })).toBe(duplicatedId);
  });

  it("falls back to memory when storage is unavailable", () => {
    const storage = {
      getItem: () => { throw new Error("blocked"); },
      setItem: () => { throw new Error("blocked"); },
    } as unknown as Storage;
    vi.stubGlobal("crypto", { randomUUID: () => "client-3" });

    const id = getClientInstanceId(storage);
    expect(getClientInstanceId(storage)).toBe(id);

    vi.unstubAllGlobals();
  });
});

function memoryStorage(): Storage {
  const items = new Map<string, string>();
  return {
    get length() {
      return items.size;
    },
    clear: () => items.clear(),
    getItem: (key) => items.get(key) ?? null,
    key: (index) => Array.from(items.keys())[index] ?? null,
    removeItem: (key) => items.delete(key),
    setItem: (key, value) => items.set(key, value),
  };
}

function copyStorage(source: Storage) {
  const copy = memoryStorage();
  for (let index = 0; index < source.length; index += 1) {
    const key = source.key(index);
    if (key) copy.setItem(key, source.getItem(key) ?? "");
  }
  return copy;
}
