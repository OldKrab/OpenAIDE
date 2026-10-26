// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ComputersSnapshot } from "../services/computers";
import type { WebviewBootstrap } from "../state/surfaceTypes";
import { useComputerPlace } from "./useComputerPlace";

const shell = { kind: "desktop", navigationMode: "project" } as const;
const startPage: Extract<WebviewBootstrap, { shell: unknown }> = { surface: "task", clientInstanceId: "c", shell };
const onStudio: ComputersSnapshot = { device: "computer", computers: [{ id: "studio", name: "Studio", current: true }] };
const onThisComputer: ComputersSnapshot = { device: "computer", computers: [{ id: "studio", name: "Studio", current: false }] };

describe("place per computer", () => {
  let container: HTMLDivElement;
  let root: Root;
  const openTask = vi.fn();
  const openNewTask = vi.fn();

  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    container = document.createElement("div");
    root = createRoot(container);
    vi.spyOn(console, "info").mockImplementation(() => undefined);
    localStorage.clear();
    restoring = [];
  });

  afterEach(() => {
    act(() => root.unmount());
    vi.clearAllMocks();
    vi.restoreAllMocks();
  });

  let restoring: boolean[] = [];

  function Probe(props: { available?: boolean; backendReady: boolean; bootstrap: WebviewBootstrap; snapshot?: ComputersSnapshot; projects?: string[] }) {
    restoring.push(useComputerPlace({
      available: props.available ?? true,
      backendReady: props.backendReady,
      bootstrap: props.bootstrap,
      hasProject: (projectId) => (props.projects ?? []).includes(projectId),
      openNewTask,
      openTask,
      snapshot: props.snapshot,
    }));
    return null;
  }
  const render = (props: Parameters<typeof Probe>[0]) => act(() => root.render(<Probe {...props} />));
  const remember = (computer: string, place: object) => localStorage.setItem(`openaide.computerPlace:${computer}`, JSON.stringify(place));

  it("returns to the last Task on the computer in use once its App Server is ready", () => {
    remember("studio", { kind: "task", taskId: "task-1" });
    remember("", { kind: "task", taskId: "task-here" });
    render({ backendReady: false, bootstrap: startPage, snapshot: onStudio });
    expect(openTask).not.toHaveBeenCalled();
    render({ backendReady: true, bootstrap: startPage, snapshot: onStudio });
    expect(openTask).toHaveBeenCalledExactlyOnceWith("task-1");
  });

  it("waits for the shell's computers before choosing", () => {
    remember("", { kind: "task", taskId: "task-here" });
    render({ backendReady: true, bootstrap: startPage });
    render({ backendReady: true, bootstrap: startPage, snapshot: onThisComputer });
    expect(openTask).toHaveBeenCalledExactlyOnceWith("task-here");
  });

  it("opens a remembered New Task Project only while the Project exists", () => {
    remember("studio", { kind: "newTask", projectId: "gone" });
    render({ backendReady: true, bootstrap: startPage, snapshot: onStudio, projects: ["kept"] });
    expect(openNewTask).not.toHaveBeenCalled();
    act(() => root.unmount());
    root = createRoot(container);
    remember("studio", { kind: "newTask", projectId: "kept" });
    render({ backendReady: true, bootstrap: startPage, snapshot: onStudio, projects: ["kept"] });
    expect(openNewTask).toHaveBeenCalledExactlyOnceWith("kept");
  });

  it("leaves a place the shell or the user already chose", () => {
    remember("studio", { kind: "task", taskId: "task-1" });
    render({ backendReady: false, bootstrap: startPage, snapshot: onStudio });
    render({ backendReady: true, bootstrap: { ...startPage, taskId: "task-2" }, snapshot: onStudio });
    expect(openTask).not.toHaveBeenCalled();
  });

  it("stays off until another computer is paired", () => {
    remember("", { kind: "task", taskId: "task-here" });
    render({ backendReady: true, bootstrap: startPage, snapshot: { device: "computer", computers: [] } });
    render({ backendReady: true, bootstrap: { ...startPage, taskId: "task-3" }, snapshot: { device: "computer", computers: [] } });
    expect(openTask).not.toHaveBeenCalled();
    expect(localStorage.getItem("openaide.computerPlace:")).toBe(JSON.stringify({ kind: "task", taskId: "task-here" }));
  });

  it("remembers Tasks and New Task Projects per computer, but never Settings", () => {
    render({ backendReady: true, bootstrap: startPage, snapshot: onStudio });
    render({ backendReady: true, bootstrap: { ...startPage, taskId: "task-4" }, snapshot: onStudio });
    render({ backendReady: true, bootstrap: { surface: "settings", clientInstanceId: "c", shell } as WebviewBootstrap, snapshot: onStudio });
    expect(JSON.parse(localStorage.getItem("openaide.computerPlace:studio")!)).toEqual({ kind: "task", taskId: "task-4" });
    render({ backendReady: true, bootstrap: { ...startPage, projectId: "p" }, snapshot: onStudio });
    expect(JSON.parse(localStorage.getItem("openaide.computerPlace:studio")!)).toEqual({ kind: "newTask", projectId: "p" });
    expect(localStorage.getItem("openaide.computerPlace:")).toBeNull();
  });

  it("holds the start page as the Task opening until it returns to a remembered Task", () => {
    remember("studio", { kind: "task", taskId: "task-1" });
    render({ backendReady: false, bootstrap: startPage });
    render({ backendReady: false, bootstrap: startPage, snapshot: onStudio });
    expect(restoring).toEqual([true, true]);
    render({ backendReady: true, bootstrap: startPage, snapshot: onStudio });
    expect(openTask).toHaveBeenCalledExactlyOnceWith("task-1");
    expect(restoring.at(-1)).toBe(false);
  });

  it("shows New Task at once when no Task is remembered for the computer in use", () => {
    remember("", { kind: "task", taskId: "task-here" });
    remember("studio", { kind: "newTask", projectId: "p" });
    render({ backendReady: false, bootstrap: startPage, snapshot: onStudio });
    expect(restoring).toEqual([false]);
  });

  it("never holds a device that saved no Task place, or a shell without computers", () => {
    render({ backendReady: false, bootstrap: startPage });
    act(() => root.unmount());
    root = createRoot(container);
    remember("studio", { kind: "task", taskId: "task-1" });
    render({ available: false, backendReady: false, bootstrap: startPage });
    expect(restoring).toEqual([false, false]);
  });

  it("stops holding once the user picks a place", () => {
    remember("studio", { kind: "task", taskId: "task-1" });
    render({ backendReady: false, bootstrap: startPage, snapshot: onStudio });
    render({ backendReady: false, bootstrap: { ...startPage, projectId: "p" }, snapshot: onStudio });
    expect(restoring).toEqual([true, false]);
  });
});
