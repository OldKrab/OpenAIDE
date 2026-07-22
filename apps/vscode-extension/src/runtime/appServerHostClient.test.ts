import { afterEach, describe, expect, it, vi } from "vitest";
import {
  AppServerProtocolError,
  ATTACHMENT_CREATE_LOCAL_FILE_REFERENCES,
  CLIENT_CAPABILITIES_CHANGED,
  CLIENT_DETACH,
  CLIENT_INITIALIZE,
  TASK_OPEN,
  type ReliableHttpFetch,
  type RpcMessage,
} from "@openaide/app-server-client";
import { AppServerHostClient } from "./appServerHostClient";

const randomMocks = vi.hoisted(() => ({
  randomUUID: vi.fn(() => "host-client-1"),
}));

vi.mock("node:crypto", () => ({
  randomUUID: randomMocks.randomUUID,
}));

describe("AppServerHostClient", () => {
  let client: AppServerHostClient | undefined;

  afterEach(() => {
    client?.dispose();
    client = undefined;
    vi.unstubAllGlobals();
    randomMocks.randomUUID.mockReturnValue("host-client-1");
  });

  it("initializes a VS Code host client over LocalHttp before typed requests", async () => {
    const fetch = fetchSequence([
      [response("rpc-1", { result: initializeResult() })],
      [response("rpc-2", { result: { task: { taskId: "task-1" } } })],
    ]);
    vi.stubGlobal("fetch", fetch);
    const provider = providerReturningConnection();
    client = new AppServerHostClient(provider);

    await client.syncWorkspaceRoots([{ path: "/workspace/app" }]);

    await expect(client.request(TASK_OPEN, { taskId: "task-1" as never }, {
      clientRequestId: "client-request-1" as never,
    })).resolves.toEqual({ task: { taskId: "task-1" } });

    const rpcCalls = jsonRpcCalls(fetch);
    expect(provider.startAppServerConnection).toHaveBeenCalledTimes(1);
    expect(provider.onAppServerConnectionChanged).toHaveBeenCalledOnce();
    expect(rpcCalls[0]).toEqual({
      jsonrpc: "2.0",
      id: "rpc-1",
      method: CLIENT_INITIALIZE,
      params: {
        clientInstanceId: "vscode-host-host-client-1",
        shell: { kind: "vscodeExtension", name: "OpenAIDE" },
        requestedSurface: { kind: "home" },
        capabilities: {
          protocol: ["requestResponses", "stableClientRequestIds", "resync"],
          shell: [
            "openExternal",
            "revealFile",
            "resolveFileReveal",
            "pickLocalFile",
            "openTerminal",
            "readSecret",
            "writeSecret",
            "showNotification",
          ],
        },
        workspaceRoots: [{ path: "/workspace/app" }],
      },
    });
    expect(rpcCalls[1]).toEqual({
      jsonrpc: "2.0",
      id: "rpc-2",
      method: TASK_OPEN,
      params: { taskId: "task-1" },
      meta: { clientRequestId: "client-request-1" },
    });
    expect(fetch.mock.calls[0]?.[1].headers["X-OpenAIDE-Connection-Id"])
      .toBe("vscode-connection-host-client-1");
  });

  it("replaces the workspace roots reported by an initialized host client", async () => {
    const fetch = fetchSequence([
      [response("rpc-1", { result: initializeResult() })],
      [response("rpc-2", { result: { projects: projectCollection() } })],
    ]);
    vi.stubGlobal("fetch", fetch);
    client = new AppServerHostClient(providerReturningConnection());

    await client.syncWorkspaceRoots([{ path: "/workspace/app" }]);
    await client.syncWorkspaceRoots([{ path: "/workspace/web" }]);

    expect(jsonRpcCalls(fetch)[1]).toEqual({
      jsonrpc: "2.0",
      id: "rpc-2",
      method: CLIENT_CAPABILITIES_CHANGED,
      params: { workspaceRoots: [{ path: "/workspace/web" }] },
    });
  });

  it("reports an empty root list when the last VS Code workspace closes", async () => {
    const fetch = fetchSequence([
      [response("rpc-1", { result: initializeResult() })],
      [response("rpc-2", { result: { projects: projectCollection() } })],
    ]);
    vi.stubGlobal("fetch", fetch);
    client = new AppServerHostClient(providerReturningConnection());

    await client.syncWorkspaceRoots([{ path: "/workspace/app" }]);
    await client.syncWorkspaceRoots([]);

    expect(jsonRpcCalls(fetch)[1]).toMatchObject({
      method: CLIENT_CAPABILITIES_CHANGED,
      params: { workspaceRoots: [] },
    });
  });

  it("explicitly detaches the VS Code host before closing its connection", async () => {
    const fetch = fetchSequence([
      [response("rpc-1", { result: initializeResult() })],
      [response("rpc-2", { result: {} })],
    ]);
    vi.stubGlobal("fetch", fetch);
    client = new AppServerHostClient(providerReturningConnection());
    await client.syncWorkspaceRoots([{ path: "/workspace/app" }]);

    await client.close();

    expect(jsonRpcCalls(fetch)[1]).toEqual({
      jsonrpc: "2.0",
      id: "rpc-2",
      method: CLIENT_DETACH,
      params: {},
    });
  });

  it("serializes a workspace replacement that races host initialization", async () => {
    const initializeResponse = deferred<RpcMessage[]>();
    const fetch = reliableFetch(async (request) => {
      if (!("method" in request) || !("id" in request)) return [];
      if (request.method === CLIENT_INITIALIZE) return initializeResponse.promise;
      return [response(request.id, { result: { projects: projectCollection() } })];
    });
    vi.stubGlobal("fetch", fetch);
    client = new AppServerHostClient(providerReturningConnection());

    const initialSync = client.syncWorkspaceRoots([{ path: "/workspace/app" }]);
    await vi.waitFor(() => expect(jsonRpcCalls(fetch)).toHaveLength(1));
    const replacementSync = client.syncWorkspaceRoots([{ path: "/workspace/web" }]);
    initializeResponse.resolve([response("rpc-1", { result: initializeResult() })]);

    await Promise.all([initialSync, replacementSync]);
    expect(jsonRpcCalls(fetch)).toMatchObject([
      {
        method: CLIENT_INITIALIZE,
        params: { workspaceRoots: [{ path: "/workspace/app" }] },
      },
      {
        method: CLIENT_CAPABILITIES_CHANGED,
        params: { workspaceRoots: [{ path: "/workspace/web" }] },
      },
    ]);
  });

  it("reuses one initialized LocalHttp connection for later typed requests", async () => {
    const fetch = fetchSequence([
      [response("rpc-1", { result: initializeResult() })],
      [response("rpc-2", { result: { task: { taskId: "task-1" } } })],
      [response("rpc-3", { result: { task: { taskId: "task-2" } } })],
    ]);
    vi.stubGlobal("fetch", fetch);
    const provider = providerReturningConnection();
    client = new AppServerHostClient(provider);

    await client.request(TASK_OPEN, { taskId: "task-1" as never });
    await client.request(TASK_OPEN, { taskId: "task-2" as never });

    expect(provider.startAppServerConnection).toHaveBeenCalledTimes(1);
    expect(jsonRpcCalls(fetch)).toHaveLength(3);
  });

  it("attaches many webview views to one initialized App Server client", async () => {
    const fetch = fetchSequence([
      [response("rpc-1", { result: initializeResult() })],
    ]);
    vi.stubGlobal("fetch", fetch);
    const provider = providerReturningConnection();
    client = new AppServerHostClient(provider);
    const navigationMessages: unknown[] = [];
    const taskMessages: unknown[] = [];
    const stopNavigation = client.attachView("navigation", (message) => {
      navigationMessages.push(message);
    });
    const stopTask = client.attachView("task-1", (message) => {
      taskMessages.push(message);
    });

    await client.handleViewMessage("navigation", {
      type: "appServer.session.initialize",
      requestId: "navigation-initialize",
    });
    await client.handleViewMessage("task-1", {
      type: "appServer.session.initialize",
      requestId: "task-initialize",
    });

    expect(provider.startAppServerConnection).toHaveBeenCalledTimes(1);
    expect(jsonRpcCalls(fetch).filter((message) => (
      "method" in message && message.method === CLIENT_INITIALIZE
    ))).toHaveLength(1);
    expect(navigationMessages).toContainEqual({
      type: "appServer.session.response",
      requestId: "navigation-initialize",
      result: initializeResult(),
    });
    expect(taskMessages).toContainEqual({
      type: "appServer.session.response",
      requestId: "task-initialize",
      result: initializeResult(),
    });

    stopNavigation();
    stopTask();
  });

  it("keeps trusted local path registration behind the extension-host boundary", async () => {
    const provider = providerReturningConnection();
    client = new AppServerHostClient(provider);
    const messages: unknown[] = [];
    client.attachView("task-1", (message) => messages.push(message));

    await client.handleViewMessage("task-1", {
      type: "appServer.session.request",
      requestId: "register-local-path",
      method: ATTACHMENT_CREATE_LOCAL_FILE_REFERENCES,
      params: { taskId: "task-1", paths: ["/outside/private.bin"] },
    });

    expect(provider.startAppServerConnection).not.toHaveBeenCalled();
    expect(messages).toEqual([{
      type: "appServer.session.response",
      requestId: "register-local-path",
      error: expect.objectContaining({ message: "This App Server method is available only to the extension host." }),
    }]);
  });

  it("surfaces protocol errors from LocalHttp typed requests", async () => {
    const fetch = fetchSequence([
      [response("rpc-1", { result: initializeResult() })],
      [{
        jsonrpc: "2.0",
        id: "rpc-2",
        error: {
          error: {
            code: "notFound",
            message: "Task not found",
            recoverable: true,
          },
        },
      }],
    ]);
    vi.stubGlobal("fetch", fetch);
    client = new AppServerHostClient(providerReturningConnection());

    const error = await client.request(TASK_OPEN, { taskId: "missing-task" as never })
      .catch((caught) => caught);

    expect(error).toBeInstanceOf(AppServerProtocolError);
    expect(error.protocolError).toMatchObject({
      code: "notFound",
      message: "Task not found",
      recoverable: true,
    });
  });
});

function providerReturningConnection() {
  return {
    startAppServerConnection: vi.fn(async () => ({
      kind: "localHttp" as const,
      endpointUrl: "http://127.0.0.1:4321/probe",
      authToken: "token-1",
    })),
    onAppServerConnectionChanged: vi.fn(() => ({ dispose: vi.fn() })),
  };
}

function fetchSequence(batches: RpcMessage[][]) {
  let nextBatch = 0;
  return reliableFetch(async () => {
    const messages = batches[nextBatch] ?? [];
    nextBatch += 1;
    return messages;
  });
}

const uploadedMessages = new WeakMap<object, RpcMessage[]>();

function reliableFetch(
  respond: (message: RpcMessage) => Promise<RpcMessage[]> | RpcMessage[],
) {
  const uploaded: RpcMessage[] = [];
  const frames: Array<{ sequence: number; message: RpcMessage }> = [];
  const pendingPolls: Array<{
    after: number;
    resolve(response: ReturnType<typeof fetchResponse>): void;
  }> = [];
  const fetch = vi.fn<ReliableHttpFetch>(async (_input, init) => {
    if (init.method === "GET") {
      const after = Number(init.headers["X-OpenAIDE-After"] ?? "0");
      const available = frames.filter((frame) => frame.sequence > after);
      if (available.length > 0) return fetchResponse({ frames: available });
      return new Promise((resolve) => pendingPolls.push({ after, resolve }));
    }
    const body = JSON.parse(init.body ?? "{}") as {
      transport?: string;
      message?: RpcMessage;
    };
    if (body.transport === "open") {
      return fetchResponse({
        transportVersion: 1,
        sessionId: "session-1",
        serverId: "server-1",
      });
    }
    if (body.transport !== "send" || !body.message) {
      return fetchResponse({}, 400);
    }
    uploaded.push(body.message);
    for (const message of await respond(body.message)) {
      frames.push({ sequence: frames.length + 1, message });
    }
    flushPolls();
    return fetchResponse({}, 204);
  });
  uploadedMessages.set(fetch, uploaded);
  return fetch;

  function flushPolls() {
    for (const poll of pendingPolls.splice(0)) {
      const available = frames.filter((frame) => frame.sequence > poll.after);
      poll.resolve(fetchResponse({ frames: available }));
    }
  }
}

function jsonRpcCalls(fetch: ReturnType<typeof fetchSequence>) {
  return uploadedMessages.get(fetch) ?? [];
}

function response(id: string | number, payload: unknown): RpcMessage {
  return {
    jsonrpc: "2.0",
    id,
    result: payload,
  };
}

function fetchResponse(body: unknown, status = 200) {
  return {
    ok: status >= 200 && status < 300,
    status,
    async text() {
      return status === 204 ? "" : JSON.stringify(body);
    },
  };
}

function initializeResult() {
  return {
    snapshot: {
      cursor: "cursor-1",
      server: { serverId: "server-1", protocolVersion: { major: 1, minor: 0 } },
      stateRoot: { stateRootId: "root-1" },
      client: {
        clientInstanceId: "vscode-host-host-client-1",
        shellKind: "vscodeExtension",
        surface: { kind: "home" },
      },
    },
  };
}

function projectCollection() {
  return { projects: [] };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}
