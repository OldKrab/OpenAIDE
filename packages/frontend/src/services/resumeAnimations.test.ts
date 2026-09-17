import { describe, expect, it } from "vitest";
import { rearmSuspendedAnimations } from "./resumeAnimations";

function fakeDocument() {
  const classes = new Set<string>();
  const documentElement = {
    classList: {
      add: (name: string) => classes.add(name),
      remove: (name: string) => classes.delete(name),
      contains: (name: string) => classes.has(name),
    },
    offsetWidth: 0,
  };
  return { classes, doc: { documentElement } as unknown as Document };
}

describe("resume animations", () => {
  it("suspends every animation for one frame and then restores it", () => {
    const { classes, doc } = fakeDocument();
    const frames: FrameRequestCallback[] = [];
    const original = globalThis.requestAnimationFrame;
    globalThis.requestAnimationFrame = ((callback: FrameRequestCallback) => {
      frames.push(callback);
      return 0;
    }) as typeof requestAnimationFrame;

    try {
      const startedAt = 1_000_000_000;
      rearmSuspendedAnimations(doc, startedAt);
      expect(classes.has("oa-animations-suspended")).toBe(true);

      frames.shift()?.(startedAt);
      expect(classes.has("oa-animations-suspended")).toBe(false);
    } finally {
      globalThis.requestAnimationFrame = original;
    }
  });

  it("ignores a resumed frame that arrives right after another one", () => {
    const { classes, doc } = fakeDocument();
    const frames: FrameRequestCallback[] = [];
    const original = globalThis.requestAnimationFrame;
    globalThis.requestAnimationFrame = ((callback: FrameRequestCallback) => {
      frames.push(callback);
      return 0;
    }) as typeof requestAnimationFrame;

    try {
      const startedAt = 2_000_000_000;
      rearmSuspendedAnimations(doc, startedAt);
      frames.shift()?.(startedAt);

      rearmSuspendedAnimations(doc, startedAt + 100);
      expect(classes.has("oa-animations-suspended")).toBe(false);

      rearmSuspendedAnimations(doc, startedAt + 6_000);
      expect(classes.has("oa-animations-suspended")).toBe(true);
    } finally {
      globalThis.requestAnimationFrame = original;
    }
  });

  it("does not crash without a document element", () => {
    expect(() => rearmSuspendedAnimations({ documentElement: null } as unknown as Document, 3_000_000_000))
      .not.toThrow();
  });
});
