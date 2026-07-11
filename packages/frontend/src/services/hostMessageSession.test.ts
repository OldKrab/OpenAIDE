import { describe, expect, it, vi } from "vitest";
import { startHostMessageSession } from "./hostMessageSession";

describe("host message session", () => {
  it("subscribes before sending startup messages", () => {
    const calls: string[] = [];
    const unsubscribe = vi.fn();
    const listener = vi.fn();
    const subscribe = vi.fn(() => {
      calls.push("subscribe");
      return unsubscribe;
    });
    const start = vi.fn(() => {
      calls.push("start");
    });

    const result = startHostMessageSession(subscribe, listener, start);

    expect(calls).toEqual(["subscribe", "start"]);
    expect(subscribe).toHaveBeenCalledWith(listener);
    expect(result).toBe(unsubscribe);
  });

  it("unsubscribes if startup messages fail", () => {
    const unsubscribe = vi.fn();
    const listener = vi.fn();
    const error = new Error("post failed");
    const subscribe = vi.fn(() => unsubscribe);
    const start = vi.fn(() => {
      throw error;
    });

    expect(() => startHostMessageSession(subscribe, listener, start)).toThrow(error);
    expect(unsubscribe).toHaveBeenCalledOnce();
  });
});
