// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { TaskStateSpinner } from "./TaskStateSpinner";

describe("TaskStateSpinner", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
  });

  afterEach(() => {
    act(() => root.unmount());
    container.remove();
    delete (HTMLElement.prototype as Partial<HTMLElement>).getAnimations;
  });

  it("starts every animation of a mounted spinner at the document timeline origin", () => {
    const animations = [{ startTime: 1234 }, { startTime: 5678 }];
    Object.defineProperty(HTMLElement.prototype, "getAnimations", { configurable: true, value: () => animations });

    act(() => root.render(<TaskStateSpinner />));

    expect(container.querySelector(".task-state-spinner")).not.toBeNull();
    expect(animations.map((animation) => animation.startTime)).toEqual([0, 0]);
  });

  it("pins the slow variant the same way", () => {
    const animations = [{ startTime: 99 }];
    Object.defineProperty(HTMLElement.prototype, "getAnimations", { configurable: true, value: () => animations });

    act(() => root.render(<TaskStateSpinner slow />));

    expect(container.querySelector(".task-state-background")).not.toBeNull();
    expect(animations[0]?.startTime).toBe(0);
  });

  it("renders without error when the Web Animations API is absent or no animation runs", () => {
    expect(() => act(() => root.render(<TaskStateSpinner />))).not.toThrow();

    Object.defineProperty(HTMLElement.prototype, "getAnimations", { configurable: true, value: () => [] });
    expect(() => act(() => root.render(<TaskStateSpinner slow />))).not.toThrow();
  });
});
