import { describe, expect, it, vi } from "vitest";
import {
  PERMISSION_REQUEST,
  QUESTION_REQUEST,
  SECRET_READ,
  SHELL_SHOW_NOTIFICATION,
  type ServerRequestMethod,
} from "@openaide/app-server-client";
import { startAppServerServerRequestBridge } from "./appServerServerRequests";

describe("App Server server-request bridge", () => {
  it("forwards shell-owned server requests to the App Shell", () => {
    const postHostMessage = vi.fn();
    const handlers = new Map<ServerRequestMethod, (params: never, context: never) => Promise<unknown>>();
    const backendConnection = {
      handleRequest: vi.fn((method, handler) => {
        handlers.set(method, handler);
        return vi.fn();
      }),
    };

    startAppServerServerRequestBridge({ backendConnection, postHostMessage });
    void handlers.get(SECRET_READ)?.({ key: "agent.secret" } as never, {
      requestId: "server-request-1", signal: new AbortController().signal,
    } as never);

    expect(postHostMessage).toHaveBeenCalledWith({
      type: "appServer.serverRequest",
      payload: {
        requestId: "server-request-1",
        method: "secret/read",
        params: { key: "agent.secret" },
      },
    });
  });

  it("responds to Backend when the App Shell returns a result", async () => {
    const postHostMessage = vi.fn();
    const handlers = new Map<ServerRequestMethod, (params: never, context: never) => Promise<unknown>>();
    const backendConnection = {
      handleRequest: vi.fn((method, handler) => {
        handlers.set(method, handler);
        return vi.fn();
      }),
    };

    const bridge = startAppServerServerRequestBridge({ backendConnection, postHostMessage });
    const response = handlers.get(SHELL_SHOW_NOTIFICATION)?.(
      { level: "info", message: "Saved" } as never,
      { requestId: "server-request-1", signal: new AbortController().signal } as never,
    );

    expect(
      bridge.handleHostMessage({
        type: "appServer.serverRequest.result",
        payload: {
          requestId: "server-request-1",
          method: SHELL_SHOW_NOTIFICATION,
          result: { actionId: null },
        },
      }),
    ).toBe(true);
    await expect(response).resolves.toEqual({ actionId: null });
  });

  it("does not respond when the App Shell result method does not match the pending request", () => {
    const postHostMessage = vi.fn();
    const handlers = new Map<ServerRequestMethod, (params: never, context: never) => Promise<unknown>>();
    const backendConnection = {
      handleRequest: vi.fn((method, handler) => {
        handlers.set(method, handler);
        return vi.fn();
      }),
    };

    const bridge = startAppServerServerRequestBridge({ backendConnection, postHostMessage });
    void handlers.get(SECRET_READ)?.({ key: "agent.secret" } as never, {
      requestId: "server-request-1", signal: new AbortController().signal,
    } as never);

    expect(
      bridge.handleHostMessage({
        type: "appServer.serverRequest.result",
        payload: {
          requestId: "server-request-1",
          method: SHELL_SHOW_NOTIFICATION,
          result: { actionId: null },
        },
      }),
    ).toBe(true);
    expect(postHostMessage).toHaveBeenCalledOnce();
  });

});
