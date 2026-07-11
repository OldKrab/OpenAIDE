import { afterEach, describe, expect, it, vi } from "vitest";
import {
  AppServerProtocolError,
  CLIENT_INITIALIZE,
  TASK_OPEN,
  type LocalHttpFetch,
} from "@openaide/app-server-client";
import { AppServerHostClient } from "./appServerHostClient";

const randomMocks = vi.hoisted(() => ({
  randomUUID: vi.fn(() => "host-client-1"),
}));

vi.mock("node:crypto", () => ({
  randomUUID: randomMocks.randomUUID,
}));

describe("AppServerHostClient", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    randomMocks.randomUUID.mockReturnValue("host-client-1");
  });

  it("initializes a VS Code host client over LocalHttp before typed requests", async () => {
    const fetch = fetchSequence([
      [response("local-http-request-1", { result: initializeResult() })],
      [response("local-http-request-2", { result: { task: { taskId: "task-1" } } })],
    ]);
    vi.stubGlobal("fetch", fetch);
    const provider = providerReturningConnection();
    const client = new AppServerHostClient(provider);

    await expect(client.request(TASK_OPEN, { taskId: "task-1" as never }, {
      clientRequestId: "client-request-1" as never,
    })).resolves.toEqual({ task: { taskId: "task-1" } });

    expect(provider.startAppServerConnection).toHaveBeenCalledTimes(1);
    expect(JSON.parse(String(fetch.mock.calls[0]?.[1].body))).toEqual({
      jsonrpc: "2.0",
      id: "local-http-request-1",
      method: CLIENT_INITIALIZE,
      params: {
        clientInstanceId: "vscode-host-host-client-1",
        shell: { kind: "vscodeExtension", name: "OpenAIDE" },
        requestedSurface: { kind: "home" },
        capabilities: { shell: ["resolveFileReveal"] },
      },
    });
    expect(JSON.parse(String(fetch.mock.calls[1]?.[1].body))).toEqual({
      jsonrpc: "2.0",
      id: "local-http-request-2",
      method: TASK_OPEN,
      params: { taskId: "task-1" },
      meta: { clientRequestId: "client-request-1" },
    });
    expect(fetch.mock.calls[0]?.[1].headers["X-OpenAIDE-Connection-Id"])
      .toBe("vscode-host-host-client-1");
  });

  it("reuses one initialized LocalHttp connection for later typed requests", async () => {
    const fetch = fetchSequence([
      [response("local-http-request-1", { result: initializeResult() })],
      [response("local-http-request-2", { result: { task: { taskId: "task-1" } } })],
      [response("local-http-request-3", { result: { task: { taskId: "task-2" } } })],
    ]);
    vi.stubGlobal("fetch", fetch);
    const provider = providerReturningConnection();
    const client = new AppServerHostClient(provider);

    await client.request(TASK_OPEN, { taskId: "task-1" as never });
    await client.request(TASK_OPEN, { taskId: "task-2" as never });

    expect(provider.startAppServerConnection).toHaveBeenCalledTimes(1);
    expect(fetch).toHaveBeenCalledTimes(3);
  });

  it("surfaces protocol errors from LocalHttp typed requests", async () => {
    const fetch = fetchSequence([
      [response("local-http-request-1", { result: initializeResult() })],
      [{
        jsonrpc: "2.0",
        id: "local-http-request-2",
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
    const client = new AppServerHostClient(providerReturningConnection());

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
  };
}

function fetchSequence(batches: unknown[][]) {
  let nextBatch = 0;
  return vi.fn<LocalHttpFetch>(async () => {
    const messages = batches[nextBatch] ?? [];
    nextBatch += 1;
    return {
      ok: true,
      status: 200,
      async text() {
        return JSON.stringify(messages);
      },
    };
  });
}

function response(id: string, payload: unknown) {
  return {
    jsonrpc: "2.0",
    id,
    ...payload as Record<string, unknown>,
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
