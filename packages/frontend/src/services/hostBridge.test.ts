import { beforeEach, describe, expect, it, vi } from "vitest";

describe("host bridge", () => {
  beforeEach(() => {
    vi.resetModules();
    vi.unstubAllGlobals();
  });

  it("creates a direct LocalHttp BackendConnection from bootstrap endpoint info", async () => {
    const fetch = reliableFetch();
    vi.stubGlobal("fetch", fetch);
    vi.stubGlobal("crypto", { randomUUID: () => "client-local" });
    vi.stubGlobal("sessionStorage", memoryStorage());
    vi.stubGlobal("document", {
      body: {
        dataset: {
          shell: "vscodeExtension",
          navigationMode: "currentProject",
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
          "X-OpenAIDE-Connection-Id": "frontend-connection-client-local",
        }),
      }),
    );
  });

  it("replaces the LocalHttp process endpoint announced by the VS Code host", async () => {
    const listeners = new Map<string, (event: { data?: unknown }) => void>();
    const fetch = replacementAwareReliableFetch();
    vi.stubGlobal("fetch", fetch);
    vi.stubGlobal("crypto", { randomUUID: () => "client-local" });
    vi.stubGlobal("sessionStorage", memoryStorage());
    vi.stubGlobal("document", {
      body: {
        dataset: {
          shell: "vscodeExtension",
          navigationMode: "currentProject",
          surface: "navigation",
          appServerConnection: JSON.stringify({
            kind: "localHttp",
            endpointUrl: "http://127.0.0.1:4321/probe",
            authToken: "token-1",
          }),
        },
      },
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    });
    vi.stubGlobal("window", {
      acquireVsCodeApi: () => ({ postMessage: vi.fn() }),
      location: { pathname: "/" },
      addEventListener: vi.fn((type: string, listener: (event: { data?: unknown }) => void) => {
        listeners.set(type, listener);
      }),
      removeEventListener: vi.fn(),
    });

    const { getBackendConnection } = await installedHostBridge();
    const connection = getBackendConnection();
    const invalidated = vi.fn();
    connection?.handleGenerationInvalidated(invalidated);
    await connection?.initialize({
      clientInstanceId: "client-local" as never,
      shell: { kind: "vscodeExtension" },
      requestedSurface: { kind: "home" },
    });
    listeners.get("message")?.({
      data: {
        type: "appServer.connectionChanged",
        payload: {
          connection: {
            kind: "localHttp",
            endpointUrl: "http://127.0.0.1:5432/probe",
            authToken: "token-2",
          },
        },
      },
    });

    await vi.waitFor(() => expect(fetch.openedEndpoints()).toEqual([
      "http://127.0.0.1:4321/probe",
      "http://127.0.0.1:5432/probe",
    ]));
    expect(invalidated).toHaveBeenCalledWith({ reason: "appServerRestarted" });
    connection?.close();
  });

  it("keeps LocalHttp connection identities distinct for task webviews sharing session storage", async () => {
    const fetch = reliableFetch();
    const sharedStorage = memoryStorage();
    sharedStorage.setItem("openaide.clientInstanceId", "shared-origin-client");
    const dataset: Record<string, string> = {
      shell: "vscodeExtension",
      navigationMode: "currentProject",
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
    first?.close();
    dataset.clientInstanceId = "task-panel-2";
    const secondBootstrap = getBootstrap();
    const second = getBackendConnection();
    await second?.initialize(initializeParamsForBootstrap(secondBootstrap));

    expect(initializeParamsForBootstrap(firstBootstrap).clientInstanceId).toBe("task-panel-1");
    expect(initializeParamsForBootstrap(secondBootstrap).clientInstanceId).toBe("task-panel-2");
    const connectionIds = fetch.mock.calls.map(([, init]) =>
      (init as { headers: Record<string, string> }).headers["X-OpenAIDE-Connection-Id"]
    );
    expect(new Set(connectionIds).size).toBe(2);
    expect(connectionIds).not.toContain("task-panel-1");
    expect(connectionIds).not.toContain("task-panel-2");
    expect(connectionIds).not.toContain("shared-origin-client");
  });

  it("turns a VS Code task adoption message into an in-place bootstrap route", async () => {
    const listeners = new Map<string, (event: { data?: unknown }) => void>();
    vi.stubGlobal("document", {
      body: {
        dataset: {
          shell: "vscodeExtension",
          navigationMode: "currentProject",
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
    const fetch = reliableFetch();
    vi.stubGlobal("fetch", fetch);
    vi.stubGlobal("crypto", { randomUUID: () => "client-web" });
    vi.stubGlobal("sessionStorage", memoryStorage());
    vi.stubGlobal("document", {
      body: {
        dataset: {
          shell: "web",
          navigationMode: "project",
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
          "X-OpenAIDE-Connection-Id": "frontend-connection-client-web",
        },
      }),
    );
  });

  it("replays queued App Server events when a suspended browser page becomes visible", async () => {
    const documentListeners = new Map<string, () => void>();
    const windowListeners = new Map<string, () => void>();
    const transport = wakeableReliableFetch();
    const documentState = {
      visibilityState: "visible",
      body: {
        dataset: {
          shell: "web",
          navigationMode: "project",
          surface: "navigation",
          appServerConnection: JSON.stringify({
            kind: "webProxy",
            endpointUrl: "/__openaide-app-server/probe",
          }),
        },
      },
      addEventListener: vi.fn((type: string, listener: () => void) => {
        documentListeners.set(type, listener);
      }),
      removeEventListener: vi.fn(),
    };
    vi.stubGlobal("fetch", transport.fetch);
    vi.stubGlobal("crypto", { randomUUID: () => "client-web" });
    vi.stubGlobal("sessionStorage", memoryStorage());
    vi.stubGlobal("document", documentState);
    vi.stubGlobal("window", {
      acquireVsCodeApi: undefined,
      location: { pathname: "/" },
      addEventListener: vi.fn((type: string, listener: () => void) => {
        windowListeners.set(type, listener);
      }),
      removeEventListener: vi.fn(),
    });
    const { getBackendConnection } = await installedHostBridge();
    const connection = getBackendConnection();
    const onEvent = vi.fn();
    connection?.handleNotification("app/event", onEvent);
    await connection?.initialize({
      clientInstanceId: "client-web" as never,
      shell: { kind: "web" },
      requestedSurface: { kind: "home" },
    });
    transport.freezeNextPoll();
    await vi.waitFor(() => expect(transport.frozenPolls()).toBe(1));
    transport.enqueue({
      jsonrpc: "2.0",
      method: "app/event",
      params: { cursor: "cursor-2" },
    });

    documentState.visibilityState = "hidden";
    documentListeners.get("visibilitychange")?.();
    documentState.visibilityState = "visible";
    documentListeners.get("visibilitychange")?.();

    await vi.waitFor(() => expect(onEvent).toHaveBeenCalledWith({ cursor: "cursor-2" }));
    expect(windowListeners.has("pageshow")).toBe(true);
    connection?.close();
  });

  it("routes web shell surface navigation through browser history without reloading", async () => {
    const pushState = vi.fn();
    const reload = vi.fn();
    const dispatchEvent = vi.fn();
    vi.stubGlobal("document", {
      body: {
        dataset: {
          shell: "web",
          navigationMode: "project",
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
          navigationMode: "project",
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
          navigationMode: "project",
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
          navigationMode: "project",
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
          navigationMode: "project",
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
          navigationMode: "project",
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
          navigationMode: "project",
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
    import("../../../../apps/browser/frontend/browserShell"),
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

function reliableFetch() {
  const queued: unknown[] = [];
  return vi.fn(async (_input: string, init?: { method?: string; body?: string }) => {
    if (init?.method === "GET") {
      const frames = queued.splice(0).map((message, index) => ({
        sequence: index + 1,
        message,
      }));
      return response(frames.length === 0 ? 204 : 200, frames.length === 0 ? "" : JSON.stringify({ frames }));
    }
    const body = init?.body ? JSON.parse(init.body) as Record<string, unknown> : {};
    if (body.transport === "open") {
      return response(200, JSON.stringify({
        transportVersion: 1,
        sessionId: "session-1",
        serverId: "server-1",
      }));
    }
    const message = body.message as { id?: string; method?: string } | undefined;
    if (message?.id && message.method === "client/initialize") {
      queued.push({
        jsonrpc: "2.0",
        id: message.id,
        result: { result: initializeResult() },
      });
    }
    return response(204, "");
  });
}

function replacementAwareReliableFetch() {
  const queued = new Map<string, unknown[]>();
  const openedEndpoints: string[] = [];
  let opened = 0;
  const fetch = vi.fn(async (input: string, init?: { method?: string; body?: string; headers?: Record<string, string> }) => {
    const body = init?.body ? JSON.parse(init.body) as Record<string, unknown> : {};
    if (body.transport === "open") {
      opened += 1;
      const sessionId = `session-${opened}`;
      openedEndpoints.push(input);
      queued.set(sessionId, []);
      return response(200, JSON.stringify({
        transportVersion: 1,
        sessionId,
        serverId: input.includes("5432") ? "server-2" : "server-1",
      }));
    }
    if (init?.method === "GET") {
      const sessionId = init.headers?.["X-OpenAIDE-Session-Id"] ?? "";
      const messages = queued.get(sessionId) ?? [];
      const frames = messages.splice(0).map((message, index) => ({ sequence: index + 1, message }));
      return response(frames.length ? 200 : 204, frames.length ? JSON.stringify({ frames }) : "");
    }
    const message = body.message as { id?: string; method?: string } | undefined;
    const sessionId = typeof body.sessionId === "string" ? body.sessionId : "";
    if (message?.id && message.method === "client/initialize") {
      queued.get(sessionId)?.push({
        jsonrpc: "2.0",
        id: message.id,
        result: {
          result: {
            ...initializeResult(),
            snapshot: {
              ...initializeResult().snapshot,
              server: {
                ...initializeResult().snapshot.server,
                serverId: sessionId === "session-2" ? "server-2" : "server-1",
              },
            },
          },
        },
      });
    }
    return response(204, "");
  });
  return Object.assign(fetch, { openedEndpoints: () => openedEndpoints });
}

function wakeableReliableFetch() {
  const queued: unknown[] = [];
  let freezeNext = false;
  let frozen = 0;
  return {
    fetch: vi.fn(async (_input: string, init?: {
      method?: string;
      body?: string;
      headers?: Record<string, string>;
      signal?: AbortSignal;
    }) => {
      if (init?.method === "GET") {
        if (freezeNext) {
          freezeNext = false;
          frozen += 1;
          return new Promise((_resolve, reject) => {
            init.signal?.addEventListener("abort", () => reject(abortError()), { once: true });
          });
        }
        const after = Number(init.headers?.["X-OpenAIDE-After"] ?? "0");
        const frames = queued
          .map((message, index) => ({ sequence: index + 1, message }))
          .filter((frame) => frame.sequence > after);
        return response(frames.length === 0 ? 204 : 200, frames.length === 0 ? "" : JSON.stringify({ frames }));
      }
      const body = init?.body ? JSON.parse(init.body) as Record<string, unknown> : {};
      if (body.transport === "open") {
        return response(200, JSON.stringify({
          transportVersion: 1,
          sessionId: "session-1",
          serverId: "server-1",
        }));
      }
      const message = body.message as { id?: string; method?: string } | undefined;
      if (message?.id && message.method === "client/initialize") {
        queued.push({
          jsonrpc: "2.0",
          id: message.id,
          result: { result: initializeResult() },
        });
      }
      return response(204, "");
    }),
    enqueue(message: unknown) {
      queued.push(message);
    },
    freezeNextPoll() {
      freezeNext = true;
    },
    frozenPolls: () => frozen,
  };
}

function response(status: number, body: string) {
  return {
    ok: status >= 200 && status < 300,
    status,
    async text() { return body; },
  };
}

function abortError() {
  const error = new Error("aborted");
  error.name = "AbortError";
  return error;
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
