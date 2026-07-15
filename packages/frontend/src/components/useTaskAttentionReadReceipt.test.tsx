import { act, create, type ReactTestRenderer } from "react-test-renderer";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { BackendConnection } from "@openaide/app-server-client";
import { useTaskAttentionReadReceipt } from "./useTaskAttentionReadReceipt";

describe("Task attention read receipt", () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("does not mark the previous Task read when focus leads to another Task", () => {
    vi.useFakeTimers();
    const windowEvents = eventTarget();
    const documentEvents = eventTarget();
    vi.stubGlobal("window", windowEvents.target);
    vi.stubGlobal("document", {
      ...documentEvents.target,
      visibilityState: "visible",
    });
    const request = vi.fn(() => new Promise(() => undefined));
    let tree: ReactTestRenderer;
    act(() => {
      tree = create(<Harness request={request} taskId="task-first" />);
    });

    windowEvents.emit("focus");
    act(() => {
      tree.update(<Harness request={request} />);
    });
    vi.runAllTimers();

    expect(request).not.toHaveBeenCalled();
    act(() => tree.unmount());
  });

  it("does not mark the previous Task read while the new route is still loading", () => {
    vi.useFakeTimers();
    const windowEvents = eventTarget();
    const documentEvents = eventTarget();
    const location = { pathname: "/task/task-first" };
    vi.stubGlobal("window", { ...windowEvents.target, location });
    vi.stubGlobal("document", {
      ...documentEvents.target,
      visibilityState: "visible",
    });
    const request = vi.fn(() => new Promise(() => undefined));
    let tree: ReactTestRenderer;
    act(() => {
      tree = create(<Harness request={request} taskId="task-first" />);
    });

    windowEvents.emit("focus");
    location.pathname = "/task/task-second";
    vi.runAllTimers();

    expect(request).not.toHaveBeenCalled();
    act(() => tree.unmount());
  });
});

function Harness({ request, taskId }: { request: ReturnType<typeof vi.fn>; taskId?: string }) {
  useTaskAttentionReadReceipt({
    backendConnection: { request } as unknown as Pick<BackendConnection, "request">,
    dispatch: vi.fn(),
    revision: 1,
    taskId,
    unread: true,
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
