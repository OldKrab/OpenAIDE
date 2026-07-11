import { beforeEach, describe, expect, it, vi } from "vitest";

describe("host bridge", () => {
  beforeEach(() => {
    vi.resetModules();
    vi.unstubAllGlobals();
  });

  it("creates a direct LocalHttp BackendConnection from bootstrap endpoint info", async () => {
    const fetch = vi.fn(async () => ({
      ok: true,
      status: 200,
      async text() {
        return JSON.stringify([
          {
            jsonrpc: "2.0",
            id: "local-http-request-1",
            result: { result: initializeResult() },
          },
        ]);
      },
    }));
    vi.stubGlobal("fetch", fetch);
    vi.stubGlobal("crypto", { randomUUID: () => "client-local" });
    vi.stubGlobal("sessionStorage", memoryStorage());
    vi.stubGlobal("document", {
      body: {
        dataset: {
          surface: "navigation",
          appServerConnection: JSON.stringify({
            kind: "localHttp",
            endpointUrl: "http://127.0.0.1:4321/probe",
            authToken: "token-1",
          }),
        },
      },
    });
    vi.stubGlobal("window", {
      acquireVsCodeApi: undefined,
      location: { pathname: "/" },
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    });

    const { getBackendConnection } = await import("./hostBridge");
    const connection = getBackendConnection();

    await connection?.initialize({
      clientInstanceId: "client-local" as never,
      shell: { kind: "web" },
      requestedSurface: { kind: "home" },
    });

    expect(fetch).toHaveBeenCalledWith(
      "http://127.0.0.1:4321/probe",
      expect.objectContaining({
        headers: expect.objectContaining({
          Authorization: "Bearer token-1",
          "X-OpenAIDE-Connection-Id": "client-local",
        }),
      }),
    );
  });

  it("creates a tokenless WebProxy BackendConnection from bootstrap endpoint info", async () => {
    const fetch = vi.fn(async () => ({
      ok: true,
      status: 200,
      async text() {
        return JSON.stringify([
          {
            jsonrpc: "2.0",
            id: "local-http-request-1",
            result: { result: initializeResult() },
          },
        ]);
      },
    }));
    vi.stubGlobal("fetch", fetch);
    vi.stubGlobal("crypto", { randomUUID: () => "client-web" });
    vi.stubGlobal("sessionStorage", memoryStorage());
    vi.stubGlobal("document", {
      body: {
        dataset: {
          surface: "navigation",
          appServerConnection: JSON.stringify({
            kind: "webProxy",
            endpointUrl: "/__openaide-app-server/probe",
          }),
        },
      },
    });
    vi.stubGlobal("window", {
      acquireVsCodeApi: undefined,
      location: { pathname: "/" },
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    });

    const { getBackendConnection } = await import("./hostBridge");
    const connection = getBackendConnection();

    await connection?.initialize({
      clientInstanceId: "client-web" as never,
      shell: { kind: "web" },
      requestedSurface: { kind: "home" },
    });

    expect(fetch).toHaveBeenCalledWith(
      "/__openaide-app-server/probe",
      expect.objectContaining({
        headers: {
          "Content-Type": "application/json",
          "X-OpenAIDE-Connection-Id": "client-web",
        },
      }),
    );
  });

  it("routes web shell surface navigation through browser history without reloading", async () => {
    const pushState = vi.fn();
    const reload = vi.fn();
    const dispatchEvent = vi.fn();
    vi.stubGlobal("document", {
      body: {
        dataset: {
          shell: "web",
          surface: "task",
          appServerConnection: JSON.stringify({
            kind: "webProxy",
            endpointUrl: "/__openaide-app-server/probe",
          }),
        },
      },
    });
    vi.stubGlobal("window", {
      acquireVsCodeApi: undefined,
      history: { pushState },
      location: { pathname: "/new-task", reload },
      addEventListener: vi.fn(),
      dispatchEvent,
      removeEventListener: vi.fn(),
    });

    const { postHostMessage } = await import("./hostBridge");
    postHostMessage({ type: "surface.openTask", payload: { task_id: "task/1" } });

    expect(pushState).toHaveBeenCalledWith(null, "", "/task/task%2F1");
    expect(reload).not.toHaveBeenCalled();
    expect(dispatchEvent).toHaveBeenCalledWith(expect.objectContaining({ type: "openaide:webRoute" }));
  });

  it("routes web shell project-scoped new task navigation through browser history", async () => {
    const pushState = vi.fn();
    vi.stubGlobal("document", {
      body: {
        dataset: {
          shell: "web",
          surface: "task",
          appServerConnection: JSON.stringify({
            kind: "webProxy",
            endpointUrl: "/__openaide-app-server/probe",
          }),
        },
      },
    });
    vi.stubGlobal("window", {
      acquireVsCodeApi: undefined,
      history: { pushState },
      location: { pathname: "/task/task_1", search: "" },
      addEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
      removeEventListener: vi.fn(),
    });

    const { postHostMessage } = await import("./hostBridge");
    postHostMessage({ type: "surface.openNewTask", payload: { project_id: "project/1" } });

    expect(pushState).toHaveBeenCalledWith(null, "", "/new-task?projectId=project%2F1");
  });

  it("routes web shell archive navigation through browser history", async () => {
    const location = { pathname: "/new-task", search: "" };
    const pushState = vi.fn((_state: unknown, _title: string, path: string) => {
      const next = new URL(path, "http://localhost");
      location.pathname = next.pathname;
      location.search = next.search;
    });
    const dispatchEvent = vi.fn();
    vi.stubGlobal("document", {
      body: {
        dataset: {
          shell: "web",
          surface: "task",
          appServerConnection: JSON.stringify({
            kind: "webProxy",
            endpointUrl: "/__openaide-app-server/probe",
          }),
        },
      },
    });
    vi.stubGlobal("window", {
      acquireVsCodeApi: undefined,
      history: { pushState },
      location,
      addEventListener: vi.fn(),
      dispatchEvent,
      removeEventListener: vi.fn(),
    });

    const { getBootstrap, postHostMessage } = await import("./hostBridge");
    postHostMessage({ type: "surface.openArchive" });

    expect(pushState).toHaveBeenCalledWith(null, "", "/archive");
    expect(dispatchEvent).toHaveBeenCalledWith(expect.objectContaining({ type: "openaide:webRoute" }));
    expect(getBootstrap()).toMatchObject({ archived: true });
  });

  it("prefers web shell path routing over injected data-surface metadata", async () => {
    vi.stubGlobal("document", {
      body: {
        dataset: {
          shell: "web",
          surface: "task",
          archived: "true",
          appServerConnection: JSON.stringify({
            kind: "webProxy",
            endpointUrl: "/__openaide-app-server/probe",
          }),
        },
      },
    });
    vi.stubGlobal("window", {
      acquireVsCodeApi: undefined,
      history: { pushState: vi.fn() },
      location: { pathname: "/archive", search: "" },
      addEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
      removeEventListener: vi.fn(),
    });

    const { getBootstrap } = await import("./hostBridge");

    expect(getBootstrap()).toMatchObject({
      surface: "navigation",
      archived: true,
      appServerConnection: { kind: "webProxy" },
    });
  });

  it("reads the selected settings tab from the web settings route", async () => {
    vi.stubGlobal("document", {
      body: {
        dataset: {
          shell: "web",
          surface: "settings",
          appServerConnection: JSON.stringify({
            kind: "webProxy",
            endpointUrl: "/__openaide-app-server/probe",
          }),
        },
      },
    });
    vi.stubGlobal("window", {
      acquireVsCodeApi: undefined,
      location: { pathname: "/settings", search: "?tab=skills" },
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    });

    const { getBootstrap } = await import("./hostBridge");

    expect(getBootstrap()).toMatchObject({
      surface: "settings",
      settingsTab: "skills",
    });
  });

  it("ignores invalid settings tab route values", async () => {
    vi.stubGlobal("document", {
      body: {
        dataset: {
          shell: "web",
          surface: "settings",
        },
      },
    });
    vi.stubGlobal("window", {
      acquireVsCodeApi: undefined,
      location: { pathname: "/settings", search: "?tab=unknown" },
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    });

    const { getBootstrap } = await import("./hostBridge");

    const bootstrap = getBootstrap();
    expect(bootstrap.surface).toBe("settings");
    if (bootstrap.surface === "settings") {
      expect(bootstrap.settingsTab).toBeUndefined();
    }
  });

  it("replaces the web settings route when a tab is selected", async () => {
    const replaceState = vi.fn();
    const dispatchEvent = vi.fn();
    vi.stubGlobal("document", {
      body: {
        dataset: {
          shell: "web",
        },
      },
    });
    vi.stubGlobal("window", {
      acquireVsCodeApi: undefined,
      history: { replaceState },
      location: { pathname: "/settings", search: "" },
      addEventListener: vi.fn(),
      dispatchEvent,
      removeEventListener: vi.fn(),
    });

    const { updateWebSettingsTabRoute } = await import("./hostBridge");
    updateWebSettingsTabRoute("skills");

    expect(replaceState).toHaveBeenCalledWith(null, "", "/settings?tab=skills");
    expect(dispatchEvent).toHaveBeenCalledWith(expect.objectContaining({ type: "openaide:webRoute" }));
  });
});

function initializeResult() {
  return {
    snapshot: {
      cursor: "cursor-1",
      server: { serverId: "server-1", protocolVersion: { major: 1, minor: 0 } },
      stateRoot: { stateRootId: "root-1" },
      client: {
        clientInstanceId: "client-local",
        shellKind: "web",
        surface: { kind: "home" },
      },
    },
  };
}

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
