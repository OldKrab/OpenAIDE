import { describe, expect, it, vi } from "vitest";
import {
  PERMISSION_REQUEST,
  QUESTION_REQUEST,
  SECRET_READ,
  SHELL_SHOW_NOTIFICATION,
  type ClientInstanceId,
  type RequestId,
  type ServerRequestMethod,
  type TaskId,
  type TypedServerRequest,
} from "@openaide/app-server-client";
import { startAppServerServerRequestBridge } from "./appServerServerRequests";

describe("App Server server-request bridge", () => {
  it("forwards shell-owned server requests to the App Shell", () => {
    const postHostMessage = vi.fn();
    let listener: ((request: TypedServerRequest<ServerRequestMethod>) => void) | undefined;
    const backendConnection = {
      serverRequests: vi.fn((nextListener) => {
        listener = nextListener as never;
        return vi.fn();
      }),
      respond: vi.fn(),
    };

    startAppServerServerRequestBridge({ backendConnection, postHostMessage });
    listener?.({
      requestId: "server-request-1" as RequestId,
      scope: { kind: "client", clientInstanceId: "client-1" as ClientInstanceId },
      method: SECRET_READ,
      params: { key: "agent.secret" },
    });

    expect(postHostMessage).toHaveBeenCalledWith({
      type: "appServer.serverRequest",
      payload: {
        requestId: "server-request-1",
        method: "secret/read",
        params: { key: "agent.secret" },
      },
    });
  });

  it("responds to Backend when the App Shell returns a result", () => {
    const postHostMessage = vi.fn();
    let listener: ((request: TypedServerRequest<ServerRequestMethod>) => void) | undefined;
    const backendConnection = {
      serverRequests: vi.fn((nextListener) => {
        listener = nextListener as never;
        return vi.fn();
      }),
      respond: vi.fn(),
    };

    const bridge = startAppServerServerRequestBridge({ backendConnection, postHostMessage });
    listener?.({
      requestId: "server-request-1" as RequestId,
      scope: { kind: "client", clientInstanceId: "client-1" as ClientInstanceId },
      method: SHELL_SHOW_NOTIFICATION,
      params: { level: "info", message: "Saved" },
    });

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
    expect(backendConnection.respond).toHaveBeenCalledWith("server-request-1", { actionId: null });
  });

  it("does not respond when the App Shell result method does not match the pending request", () => {
    const postHostMessage = vi.fn();
    let listener: ((request: TypedServerRequest<ServerRequestMethod>) => void) | undefined;
    const backendConnection = {
      serverRequests: vi.fn((nextListener) => {
        listener = nextListener as never;
        return vi.fn();
      }),
      respond: vi.fn(),
    };

    const bridge = startAppServerServerRequestBridge({ backendConnection, postHostMessage });
    listener?.({
      requestId: "server-request-1" as RequestId,
      scope: { kind: "client", clientInstanceId: "client-1" as ClientInstanceId },
      method: SECRET_READ,
      params: { key: "agent.secret" },
    });

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
    expect(backendConnection.respond).not.toHaveBeenCalled();
  });

});
