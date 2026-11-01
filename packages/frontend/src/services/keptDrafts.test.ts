// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from "vitest";
import { dropKeptDrafts, keepDrafts, takeKeptDrafts } from "./keptDrafts";

const FAR_FUTURE = new Date("2999-01-01").getTime();

describe("kept drafts", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    localStorage.clear();
  });

  it("hands a computer its own drafts once", () => {
    keepDrafts("studio", { newTask: "new", tasks: { "task-1": "first" } });
    keepDrafts("laptop", { tasks: { "task-2": "second" } });
    expect(takeKeptDrafts("studio")).toEqual({ newTask: "new", tasks: { "task-1": "first" } });
    expect(takeKeptDrafts("studio")).toBeUndefined();
    expect(takeKeptDrafts("laptop")).toEqual({ tasks: { "task-2": "second" } });
  });

  it("clears an older copy when nothing is left to keep", () => {
    keepDrafts("studio", { tasks: { "task-1": "first" } });
    expect(keepDrafts("studio", { tasks: {} })).toBe(true);
    expect(takeKeptDrafts("studio")).toBeUndefined();
  });

  it("forgets a dropped copy", () => {
    keepDrafts("studio", { tasks: { "task-1": "first" } });
    dropKeptDrafts("studio");
    expect(takeKeptDrafts("studio")).toBeUndefined();
  });

  it("ends every entry past its retention, even for computers not opened", () => {
    keepDrafts("studio", { tasks: { "task-1": "first" } });
    keepDrafts("laptop", { tasks: { "task-2": "second" } });
    expect(takeKeptDrafts("studio", localStorage, FAR_FUTURE)).toBeUndefined();
    expect(localStorage.length).toBe(0);
  });

  it("reports refused storage so the switch can ask", () => {
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => { throw new DOMException("full", "QuotaExceededError"); });
    expect(keepDrafts("studio", { tasks: { "task-1": "first" } })).toBe(false);
  });

  it("treats a corrupt entry as nothing kept and removes it", () => {
    vi.spyOn(console, "warn").mockImplementation(() => undefined);
    localStorage.setItem("openaide.keptDrafts:studio", "{");
    expect(takeKeptDrafts("studio")).toBeUndefined();
    expect(localStorage.length).toBe(0);
  });
});
