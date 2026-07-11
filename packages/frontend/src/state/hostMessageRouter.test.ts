import { describe, expect, it, vi } from "vitest";
import type { HostToWebviewMessage } from "@openaide/app-shell-contracts";
import { routeHostMessage } from "./hostMessageRouter";
import { SnapshotRequestTracker } from "./snapshotRequests";
import type { HostMessageRouterContext } from "./hostMessageRouterTypes";

describe("host message router", () => {
  it("routes shell-local settings messages", () => {
    const context = routerContext();

    routeHostMessage(
      {
        type: "runtime.settings.result",
        payload: { developer: { acp_trace: { enabled: true, directory: "/runtime/traces" } } },
      },
      context,
    );
    routeHostMessage(
      {
        type: "workspace.roots.result",
        payload: { roots: [{ path: "/workspace/app", label: "app" }] },
      },
      context,
    );

    expect(context.dispatch).toHaveBeenCalledWith({
      type: "settings:runtimeSettings",
      settings: { developer: { acp_trace: { enabled: true, directory: "/runtime/traces" } } },
    });
    expect(context.dispatch).toHaveBeenCalledWith({
      type: "workspace:roots",
      roots: [{ path: "/workspace/app", label: "app" }],
    });
  });

  it("routes shell-local surface navigation", () => {
    const context = routerContext();

    routeHostMessage({ type: "newTask" }, context);
    routeHostMessage({ type: "showSettings" }, context);

    expect(context.postHostMessage).toHaveBeenCalledWith({ type: "surface.openNewTask" });
    expect(context.postHostMessage).toHaveBeenCalledWith({ type: "surface.openSettings" });
  });

  it("routes secret storage failures back to Agent Settings", () => {
    const context = routerContext();

    routeHostMessage({
      type: "runtime.error",
      payload: {
        action: "secret.transaction.apply",
        message: "Secret storage is unavailable",
      },
    }, context);

    expect(context.dispatch).toHaveBeenCalledWith({
      type: "settings:error",
      message: "Secret storage is unavailable",
    });
  });

});

function routerContext(overrides: Partial<HostMessageRouterContext> = {}) {
  return { ...routerContextBase(), ...overrides };
}

function routerContextBase() {
  return {
    bootstrap: { surface: "navigation" as const },
    dispatch: vi.fn(),
    setAgents: vi.fn(),
    setPreferences: vi.fn(),
    snapshotRequests: { current: new SnapshotRequestTracker() },
    latestOptionsRequestKey: { current: undefined as string | undefined },
    latestSessionListRequestId: { current: undefined as number | undefined },
    nextSessionListRequestId: { current: 0 },
    latestNativeSessionSelection: { current: { agentId: "codex", workspaceRoot: "/workspace/app" } },
    createSnapshotRequestId: vi.fn(() => 1),
    postHostMessage: vi.fn(),
  };
}
