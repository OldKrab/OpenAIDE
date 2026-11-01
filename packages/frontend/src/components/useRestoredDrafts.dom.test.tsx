// @vitest-environment jsdom

import { act, type Dispatch } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ComputersCapability, ComputersSnapshot } from "../services/computers";
import { keepDrafts } from "../services/keptDrafts";
import type { AppAction } from "../state/appReducer";
import { useRestoredDrafts } from "./useRestoredDrafts";

const emptyInputs = { newTask: { prompt: "" }, taskInputs: {} } as never;

function computers(snapshot: ComputersSnapshot | undefined): ComputersCapability {
  return {
    snapshot: () => snapshot,
    subscribe: () => () => undefined,
    use: async () => "switched",
    forget: async () => undefined,
  };
}

const onStudio: ComputersSnapshot = { device: "computer", computers: [{ id: "studio", name: "Studio", current: true }] };

describe("useRestoredDrafts", () => {
  let root: Root;

  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    root = createRoot(document.createElement("div"));
    vi.spyOn(console, "info").mockImplementation(() => undefined);
  });

  afterEach(() => {
    act(() => root.unmount());
    vi.restoreAllMocks();
    localStorage.clear();
  });

  function Probe(props: Parameters<typeof useRestoredDrafts>[0]) {
    useRestoredDrafts(props);
    return null;
  }

  function restore(capability: ComputersCapability, backendReady: boolean, latest = emptyInputs) {
    const dispatch = vi.fn<Dispatch<AppAction>>();
    const latestState = { current: latest };
    const render = (ready: boolean) => act(() => root.render(<Probe backendReady={ready} capability={capability} dispatch={dispatch} latestState={latestState} />));
    render(backendReady);
    return { dispatch, render };
  }

  it("puts kept text back once the App Server is ready, and only once", () => {
    keepDrafts("studio", { newTask: "new", tasks: { "task-1": "first" } });
    const { dispatch, render } = restore(computers(onStudio), false);
    expect(dispatch).not.toHaveBeenCalled();
    render(true);
    expect(dispatch.mock.calls.map(([action]) => action)).toEqual([
      { type: "prompt", prompt: "new" },
      { type: "taskInput:prompt", taskId: "task-1", prompt: "first" },
    ]);
    render(true);
    expect(dispatch).toHaveBeenCalledTimes(2);
    expect(vi.mocked(console.info)).toHaveBeenCalledWith("computer_switch_drafts_restored count=2");
  });

  it("leaves text typed in the meantime alone", () => {
    keepDrafts("studio", { tasks: { "task-1": "first", "task-2": "second" } });
    const { dispatch } = restore(computers(onStudio), true, { newTask: { prompt: "" }, taskInputs: { "task-1": { prompt: "newer" } } } as never);
    expect(dispatch.mock.calls.map(([action]) => action)).toEqual([{ type: "taskInput:prompt", taskId: "task-2", prompt: "second" }]);
  });

  it("restores nothing for another computer", () => {
    keepDrafts("laptop", { tasks: { "task-1": "first" } });
    const { dispatch } = restore(computers(onStudio), true);
    expect(dispatch).not.toHaveBeenCalled();
    expect(localStorage.length).toBe(1);
  });

  it("restores this device's drafts when no paired computer is current", () => {
    keepDrafts("", { tasks: { "task-1": "first" } });
    const { dispatch } = restore(computers({ device: "phone", computers: [{ id: "studio", name: "Studio", current: false }] }), true);
    expect(dispatch).toHaveBeenCalledWith({ type: "taskInput:prompt", taskId: "task-1", prompt: "first" });
  });
});
