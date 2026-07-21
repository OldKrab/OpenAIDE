import { beforeEach, describe, expect, it, vi } from "vitest";
import { AppServerProtocolError, TASK_ADOPT_NATIVE_SESSION, TASK_RELEASE } from "@openaide/app-server-client";
import { AsyncOperationOwner } from "../state/asyncOperationOwner";
import { createInitialState } from "../state/store";
import { ComposerAttachmentResourceOwner } from "../services/attachmentResources";
import { NewTaskController } from "./newTaskController";

const { openTaskSurface, postHostMessage } = vi.hoisted(() => ({
  openTaskSurface: vi.fn(),
  postHostMessage: vi.fn(),
}));
vi.mock("../services/hostBridge", () => ({ openTaskSurface, postHostMessage }));
vi.mock("../state/appServerProtocolMapping", () => ({
  mapProtocolTaskSnapshot: () => ({
    snapshot: {
      task: { task_id: "task-adopted", title: "Adopted session" },
    },
  }),
}));

import { adoptRoutedNativeSession } from "./useNativeSessionRouteLifecycle";

describe("Native Session route lifecycle", () => {
  beforeEach(() => {
    openTaskSurface.mockReset();
    postHostMessage.mockReset();
  });

  it("adopts from route identity and replaces the route with the created Task", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async () => ({ task: {} }));

    await adoptRoutedNativeSession(routeLifecycle({ dispatch, request }));

    expect(request).toHaveBeenCalledWith(TASK_ADOPT_NATIVE_SESSION, {
      agentId: "codex",
      nativeSessionId: "session-1",
    });
    expect(dispatch).toHaveBeenNthCalledWith(1, {
      type: "newTask:nativeSessions:adopt",
      sessionId: "session-1",
    });
    expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({ type: "snapshot", intent: "open" }));
    expect(openTaskSurface).toHaveBeenCalledWith("task-adopted", "Adopted session");
  });

  it("discards a Prepared Task before loading the routed Native Session", async () => {
    const calls: string[] = [];
    const request = vi.fn(async (method: string) => {
      calls.push(method);
      return method === TASK_RELEASE ? { discarded: true } : { task: {} };
    });
    const state = preparedState();
    const newTaskController = new NewTaskController();
    newTaskController.claim({ preparationKey: "codex\u0000project-1", taskId: "task-prepared" as never });

    await adoptRoutedNativeSession(routeLifecycle({ request, state, newTaskController }));

    expect(calls).toEqual([TASK_RELEASE, TASK_ADOPT_NATIVE_SESSION]);
  });

  it("keeps the opening route, reports not-found, and removes the stale row", async () => {
    const dispatch = vi.fn();
    const request = vi.fn(async () => {
      throw new AppServerProtocolError({
        error: { code: "notFound", message: "missing", recoverable: false },
      });
    });

    await adoptRoutedNativeSession(routeLifecycle({ dispatch, request }));

    expect(dispatch).toHaveBeenCalledWith({
      type: "newTask:nativeSessions:error",
      sessionId: "session-1",
      message: "This session no longer exists.",
    });
    expect(dispatch).toHaveBeenCalledWith({
      type: "newTask:nativeSessions:remove",
      sessionId: "session-1",
    });
    expect(openTaskSurface).not.toHaveBeenCalled();
  });

  it("does not let a completed load replace a newer route", async () => {
    let resolve!: (value: { task: object }) => void;
    const request = vi.fn(() => new Promise<{ task: object }>((settle) => { resolve = settle; }));
    const asyncOperations = new AsyncOperationOwner();
    const lifecycle = routeLifecycle({ asyncOperations, request });
    const adoption = adoptRoutedNativeSession(lifecycle);
    await Promise.resolve();
    asyncOperations.beginNavigation("settings:default");
    resolve({ task: {} });

    await adoption;

    expect(openTaskSurface).not.toHaveBeenCalled();
  });
});

function routeLifecycle(overrides: Record<string, unknown> = {}) {
  return {
    agentId: "codex",
    asyncOperations: new AsyncOperationOwner(),
    attachmentResources: new ComposerAttachmentResourceOwner({ release: vi.fn() }),
    backendConnection: { request: vi.fn(async () => ({ task: {} })) },
    dispatch: vi.fn(),
    nativeSessionId: "session-1",
    newTaskController: new NewTaskController(),
    routeKey: "0\u0000codex\u0000session-1",
    state: createInitialState(),
    ...overrides,
    ...(overrides.request ? { backendConnection: { request: overrides.request } } : {}),
  } as unknown as Parameters<typeof adoptRoutedNativeSession>[0];
}

function preparedState() {
  const state = createInitialState();
  state.newTask.selection = {
    ...state.newTask.selection,
    agentId: "codex",
    projectId: "project-1",
    workspaceRoot: "/workspace",
  };
  state.snapshot = {
    lifecycle: "new",
    task: { task_id: "task-prepared" },
  } as never;
  return state;
}
