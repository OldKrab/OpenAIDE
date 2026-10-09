// timing-file: mocked — the ask interval and gap run on Vitest fake timers.

import { act, create, type ReactTestRenderer } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AGENT_REFRESH_ACCOUNT_LIMITS, type BackendConnection } from "@openaide/app-server-client";
import { useAccountLimitsRefresh } from "./useAccountLimitsRefresh";

describe("Account limits refresh", () => {
  let windowEvents: ReturnType<typeof eventTarget>;
  let documentState: { visibilityState: "visible" | "hidden" };
  let request: ReturnType<typeof vi.fn>;
  let tree: ReactTestRenderer;

  beforeEach(() => {
    vi.useFakeTimers();
    windowEvents = eventTarget();
    const documentEvents = eventTarget();
    documentState = Object.assign(documentEvents.target, { visibilityState: "visible" as const });
    vi.stubGlobal("window", windowEvents.target);
    vi.stubGlobal("document", documentState);
    request = vi.fn(async () => ({ started: true }));
  });

  afterEach(() => {
    act(() => tree?.unmount());
    tree = undefined as unknown as ReactTestRenderer;
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  const render = (agentId?: string) => {
    act(() => {
      const element = <Harness agentId={agentId} request={request} />;
      if (tree) tree.update(element);
      else tree = create(element);
    });
  };

  it("asks when a Task opens and again while it stays in view", () => {
    render("claude-code");
    expect(request).toHaveBeenCalledExactlyOnceWith(AGENT_REFRESH_ACCOUNT_LIMITS, { agentId: "claude-code" });

    vi.advanceTimersByTime(5 * 60_000);
    expect(request).toHaveBeenCalledTimes(2);
  });

  it("asks on returning to the page, but not on every hop", () => {
    render("claude-code");
    windowEvents.emit("focus");
    expect(request).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(61_000);
    windowEvents.emit("focus");
    expect(request).toHaveBeenCalledTimes(2);
  });

  it("stays quiet while the page is hidden or no Task is open", () => {
    render();
    expect(request).not.toHaveBeenCalled();

    documentState.visibilityState = "hidden";
    render("claude-code");
    vi.advanceTimersByTime(10 * 60_000);
    expect(request).not.toHaveBeenCalled();
  });

  it("asks for the new Agent when the Task in view changes and survives a failed request", async () => {
    request.mockRejectedValueOnce(new Error("offline"));
    render("claude-code");
    await act(async () => {});
    render("codex");
    expect(request).toHaveBeenLastCalledWith(AGENT_REFRESH_ACCOUNT_LIMITS, { agentId: "codex" });
  });
});

function Harness({ agentId, request }: { agentId?: string; request: ReturnType<typeof vi.fn> }) {
  useAccountLimitsRefresh({
    agentId,
    backendConnection: { request } as unknown as Pick<BackendConnection, "request">,
  });
  return null;
}

function eventTarget() {
  const listeners = new Map<string, Set<EventListener>>();
  return {
    target: {
      addEventListener(type: string, listener: EventListener) {
        const registered = listeners.get(type) ?? new Set<EventListener>();
        registered.add(listener);
        listeners.set(type, registered);
      },
      removeEventListener(type: string, listener: EventListener) {
        listeners.get(type)?.delete(listener);
      },
    },
    emit(type: string) {
      for (const listener of listeners.get(type) ?? []) listener(new Event(type));
    },
  };
}
