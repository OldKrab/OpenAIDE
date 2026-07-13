import { beforeEach, describe, expect, it, vi } from "vitest";

describe("host bridge", () => {
  beforeEach(() => {
    vi.resetModules();
    vi.unstubAllGlobals();
  });

  it("creates a direct LocalHttp BackendConnection from bootstrap endpoint info", async () => {
    const fetch = vi.fn(async (_input: string, _init?: unknown) => ({
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

    const { getBackendConnection } = await installedHostBridge();
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

  it("keeps LocalHttp connection identities distinct for task webviews sharing session storage", async () => {
    const fetch = vi.fn(async (_input: string, _init?: unknown) => ({
      ok: true,
      status: 200,
      async text() {
        return JSON.stringify([{
          jsonrpc: "2.0",
          id: "local-http-request-1",
          result: { result: initializeResult() },
        }]);
      },
    }));
    const sharedStorage = memoryStorage();
    sharedStorage.setItem("openaide.clientInstanceId", "shared-origin-client");
    const dataset: Record<string, string> = {
      surface: "task",
      clientInstanceId: "task-panel-1",
      appServerConnection: JSON.stringify({
        kind: "localHttp",
        endpointUrl: "http://127.0.0.1:4321/probe",
        authToken: "token-1",
      }),
    };
    vi.stubGlobal("fetch", fetch);
    vi.stubGlobal("sessionStorage", sharedStorage);
    vi.stubGlobal("document", { body: { dataset } });
    vi.stubGlobal("window", {
      acquireVsCodeApi: undefined,
      location: { pathname: "/" },
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    });

    const { getBackendConnection, getBootstrap } = await installedHostBridge();
    const { initializeParamsForBootstrap } = await import("./backendInitialization");
    const firstBootstrap = getBootstrap();
    const first = getBackendConnection();
    await first?.initialize(initializeParamsForBootstrap(firstBootstrap));
    dataset.clientInstanceId = "task-panel-2";
    const secondBootstrap = getBootstrap();
    const second = getBackendConnection();
    await second?.initialize(initializeParamsForBootstrap(secondBootstrap));

    expect(initializeParamsForBootstrap(firstBootstrap).clientInstanceId).toBe("task-panel-1");
    expect(initializeParamsForBootstrap(secondBootstrap).clientInstanceId).toBe("task-panel-2");
    const connectionIds = fetch.mock.calls.map(([, init]) =>
      (init as { headers: Record<string, string> }).headers["X-OpenAIDE-Connection-Id"]
    );
    expect(connectionIds).toContain("task-panel-1");
    expect(connectionIds).toContain("task-panel-2");
    expect(connectionIds).not.toContain("shared-origin-client");
  });

  it("turns a VS Code task adoption message into an in-place bootstrap route", async () => {
    const listeners = new Map<string, (event: { data?: unknown }) => void>();
    vi.stubGlobal("document", {
      body: {
        dataset: {
          surface: "task",
          clientInstanceId: "task-panel-1",
          appServerConnection: JSON.stringify({
            kind: "localHttp",
            endpointUrl: "http://127.0.0.1:4321/probe",
            authToken: "token-1",
          }),
        },
      },
    });
    vi.stubGlobal("window", {
      acquireVsCodeApi: () => ({ postMessage: vi.fn() }),
      location: { pathname: "/" },
      addEventListener: vi.fn((type: string, listener: (event: { data?: unknown }) => void) => {
        listeners.set(type, listener);
      }),
      removeEventListener: vi.fn(),
    });

    const { subscribeSurfaceRouteChanges } = await installedHostBridge();
    const routed = vi.fn();
    subscribeSurfaceRouteChanges(routed);
    listeners.get("message")?.({
      data: {
        type: "surface.routeChanged",
        payload: { surface: "task", task_id: "created_task" },
      },
    });

    expect(routed).toHaveBeenCalledWith(expect.objectContaining({
      surface: "task",
      taskId: "created_task",
      clientInstanceId: "task-panel-1",
      appServerConnection: expect.objectContaining({ kind: "localHttp" }),
    }));
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

    const { getBackendConnection } = await installedHostBridge();
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

    const { openTaskSurface } = await installedHostBridge();
    openTaskSurface("task/1");

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

    const { openNewTaskSurface } = await installedHostBridge();
    openNewTaskSurface("project/1");

    expect(pushState).toHaveBeenCalledWith(null, "", "/new-task?projectId=project%2F1");
  });

  it("reads archive navigation from browser history", async () => {
    const location = { pathname: "/new-task", search: "" };
    const pushState = vi.fn((_state: unknown, _title: string, path: string) => {
      const next = new URL(path, "http://localhost");
      location.pathname = next.pathname;
      location.search = next.search;
    });
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
      dispatchEvent: vi.fn(),
      removeEventListener: vi.fn(),
    });

    const { getBootstrap } = await installedHostBridge();
    window.history.pushState(null, "", "/archive");

    expect(pushState).toHaveBeenCalledWith(null, "", "/archive");
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

    const { getBootstrap } = await installedHostBridge();

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

    const { getBootstrap } = await installedHostBridge();

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

    const { getBootstrap } = await installedHostBridge();

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

    const { replaceSettingsTabRoute } = await installedHostBridge();
    replaceSettingsTabRoute("skills");

    expect(replaceState).toHaveBeenCalledWith(null, "", "/settings?tab=skills");
    expect(dispatchEvent).toHaveBeenCalledWith(expect.objectContaining({ type: "openaide:webRoute" }));
  });
});

async function installedHostBridge() {
  const [{ installFrontendShell }, { createBrowserShell }, hostBridge] = await Promise.all([
    import("./frontendShell"),
    import("../shells/browserShell"),
    import("./hostBridge"),
  ]);
  installFrontendShell(createBrowserShell());
  return hostBridge;
}

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
