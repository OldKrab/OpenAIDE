import { describe, expect, it, vi } from "vitest";
import type { ClientInstanceId } from "@openaide/app-server-client";
import { getClientInstanceId, initializeParamsForBootstrap } from "./backendInitialization";

describe("backend initialization", () => {
  it("builds initialize params from the current shell surface", () => {
    expect(initializeParamsForBootstrap(
      { surface: "navigation" },
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
          "permissionResponses",
          "questionResponses",
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
      { surface: "task", taskId: "task-1" },
      "client-1" as ClientInstanceId,
    ).requestedSurface).toEqual({ kind: "task", taskId: "task-1" });

    expect(initializeParamsForBootstrap(
      { surface: "task" },
      "client-1" as ClientInstanceId,
    ).requestedSurface).toEqual({ kind: "newTask" });

    expect(initializeParamsForBootstrap(
      { surface: "task", projectId: "project-1" },
      "client-1" as ClientInstanceId,
    ).requestedSurface).toEqual({ kind: "newTask", projectId: "project-1" });

    expect(initializeParamsForBootstrap(
      { surface: "settings" },
      "client-1" as ClientInstanceId,
    ).requestedSurface).toEqual({ kind: "settings" });
  });

  it("identifies web proxy bootstrap as the web shell", () => {
    const initialized = initializeParamsForBootstrap(
      {
        surface: "task",
        appServerConnection: {
          kind: "webProxy",
          endpointUrl: "/__openaide-app-server/probe",
        },
      },
      "client-web" as ClientInstanceId,
    );

    expect(initialized.shell).toEqual({ kind: "web" });
    expect(initialized.capabilities?.shell ?? []).not.toContain("readSecret");
    expect(initialized.capabilities?.shell ?? []).not.toContain("writeSecret");
  });

  it("uses session storage for stable tab identity", () => {
    const storage = memoryStorage();
    vi.stubGlobal("crypto", { randomUUID: () => "client-2" });

    expect(getClientInstanceId(storage)).toBe("client-2");
    expect(getClientInstanceId(storage)).toBe("client-2");
    expect(storage.getItem("openaide.clientInstanceId")).toBe("client-2");

    vi.unstubAllGlobals();
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
