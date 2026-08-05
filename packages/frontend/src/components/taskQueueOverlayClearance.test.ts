import { afterEach, describe, expect, it, vi } from "vitest";
import { installTaskQueueOverlayClearance } from "./taskQueueOverlayClearance";

describe("Task Queue overlay clearance", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("adds only the expanded height to the Chat scroll end and clears it on collapse", () => {
    let resize!: ResizeObserverCallback;
    const disconnect = vi.fn();
    vi.stubGlobal("ResizeObserver", class {
      constructor(callback: ResizeObserverCallback) { resize = callback; }
      observe = vi.fn();
      unobserve = vi.fn();
      disconnect = disconnect;
    });

    const properties = new Map<string, string>();
    const conversation = {
      style: {
        getPropertyValue: (name: string) => properties.get(name) ?? "",
        removeProperty: (name: string) => properties.delete(name),
        setProperty: (name: string, value: string) => properties.set(name, value),
      },
    } as unknown as HTMLElement;
    let height = 214;
    const floating = {
      closest: () => conversation,
      getBoundingClientRect: () => ({ height }),
    } as unknown as HTMLDivElement;

    const cleanup = installTaskQueueOverlayClearance(floating);
    expect(conversation.style.getPropertyValue("--task-queue-overlay-clearance")).toBe("180px");

    height = 34;
    resize([], {} as ResizeObserver);
    expect(conversation.style.getPropertyValue("--task-queue-overlay-clearance")).toBe("0px");

    cleanup();
    expect(disconnect).toHaveBeenCalledOnce();
    expect(conversation.style.getPropertyValue("--task-queue-overlay-clearance")).toBe("");
  });
});
