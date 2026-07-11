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

  it("maps permission server requests to the task UI flow", () => {
    const postHostMessage = vi.fn();
    const onPermissionRequest = vi.fn();
    let listener: ((request: TypedServerRequest<ServerRequestMethod>) => void) | undefined;
    const backendConnection = {
      serverRequests: vi.fn((nextListener) => {
        listener = nextListener as never;
        return vi.fn();
      }),
      respond: vi.fn(),
    };

    startAppServerServerRequestBridge({ backendConnection, onPermissionRequest, postHostMessage });
    listener?.({
      requestId: "server-request-1" as RequestId,
      scope: { kind: "task", taskId: "task_1" as TaskId },
      method: PERMISSION_REQUEST,
      params: {
        title: "Allow tool?",
        toolCall: { id: "tool-1", title: "Edit" },
        options: [{ optionId: "allow", name: "Allow", kind: "allowOnce" }],
      },
    });

    expect(postHostMessage).not.toHaveBeenCalled();
    expect(backendConnection.respond).not.toHaveBeenCalled();
    expect(onPermissionRequest).toHaveBeenCalledWith(
      "server-request-1",
      expect.objectContaining({
        message_type: "permission",
        message: expect.objectContaining({
          kind: "permission",
          app_server_request_id: "server-request-1",
          options: [{ id: "allow", label: "Allow", kind: "allow" }],
          state: "pending",
          title: "Allow tool?",
        }),
      }),
      "task_1",
    );
  });

  it("maps Question server requests to the bound Task UI flow", () => {
    const postHostMessage = vi.fn();
    const onQuestionRequest = vi.fn();
    let listener: ((request: TypedServerRequest<ServerRequestMethod>) => void) | undefined;
    const backendConnection = {
      serverRequests: vi.fn((nextListener) => {
        listener = nextListener as never;
        return vi.fn();
      }),
      respond: vi.fn(),
    };

    startAppServerServerRequestBridge({ backendConnection, onQuestionRequest, postHostMessage });
    listener?.({
      requestId: "question-1" as RequestId,
      scope: { kind: "task", taskId: "task_1" as TaskId },
      method: QUESTION_REQUEST,
      params: {
        message: "Choose a scope.",
        fields: [{
          kind: "singleSelect",
          key: "scope",
          title: "Scope",
          required: true,
          options: [{ value: "form", label: "Form only" }],
        }],
      },
    });

    expect(postHostMessage).not.toHaveBeenCalled();
    expect(onQuestionRequest).toHaveBeenCalledWith(
      "question-1",
      expect.objectContaining({
        message_type: "elicitation",
        message: expect.objectContaining({
          kind: "elicitation",
          app_server_request_id: "question-1",
          prompt: "Choose a scope.",
          state: "pending",
        }),
      }),
      "task_1",
    );
  });
});
