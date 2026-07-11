import { describe, expect, it, vi } from "vitest";
import {
  postControllerStarted,
  postStartupRequests,
} from "./appControllerEffects";

describe("app controller lifecycle effects", () => {
  it("posts startup telemetry with the initialized surface", () => {
    const postHostMessage = vi.fn();

    postControllerStarted(postHostMessage, { surface: "task", taskId: "task_1" });

    expect(postHostMessage).toHaveBeenCalledWith({
      type: "webview.telemetry",
      payload: {
        event: "started",
        surface: "task",
        task_id: "task_1",
      },
    });
  });

  it("starts navigation with workspace-root request and local task-list error", () => {
    const postHostMessage = vi.fn();
    const dispatchNavigationError = vi.fn();

    postStartupRequests({
      bootstrap: { surface: "navigation" },
      dispatchNavigationError,
      dispatchSettingsStart: vi.fn(),
      dispatchSettingsError: vi.fn(),
      dispatchTaskOpenError: vi.fn(),
      postHostMessage,
    });

    expect(postHostMessage).toHaveBeenCalledWith({ type: "workspace.roots" });
    expect(dispatchNavigationError).toHaveBeenCalledWith("App Server connection unavailable.");
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "task.list" }));
  });

  it("starts task surfaces with a local App Server connection error", () => {
    const postHostMessage = vi.fn();
    const dispatchTaskOpenError = vi.fn();

    postStartupRequests({
      bootstrap: { surface: "task", taskId: "task_1" },
      dispatchNavigationError: vi.fn(),
      dispatchSettingsStart: vi.fn(),
      dispatchSettingsError: vi.fn(),
      dispatchTaskOpenError,
      postHostMessage,
    });

    expect(dispatchTaskOpenError).toHaveBeenCalledWith("task_1", "App Server connection unavailable.");
    expect(postHostMessage).not.toHaveBeenCalledWith(expect.objectContaining({ type: "task.markRead" }));
  });

  it("starts settings with pending state before reporting a local connection error", () => {
    const calls: string[] = [];
    const dispatchSettingsStart = vi.fn(() => calls.push("settings:start"));
    const dispatchSettingsError = vi.fn(() => calls.push("settings:error"));
    const postHostMessage = vi.fn();

    postStartupRequests({
      bootstrap: { surface: "settings" },
      dispatchNavigationError: vi.fn(),
      dispatchSettingsStart,
      dispatchSettingsError,
      dispatchTaskOpenError: vi.fn(),
      postHostMessage,
    });

    expect(calls.indexOf("settings:start")).toBeLessThan(calls.indexOf("settings:error"));
    expect(dispatchSettingsError).toHaveBeenCalledWith("App Server connection unavailable.");
    expect(postHostMessage).not.toHaveBeenCalledWith({ type: "settings.snapshot" });
  });

  it("skips legacy settings snapshot startup when settings reads are handled elsewhere", () => {
    const dispatchSettingsStart = vi.fn();
    const postHostMessage = vi.fn();

    postStartupRequests({
      bootstrap: { surface: "settings" },
      dispatchNavigationError: vi.fn(),
      dispatchSettingsStart,
      dispatchSettingsError: vi.fn(),
      dispatchTaskOpenError: vi.fn(),
      skipSettingsReadRequests: true,
      postHostMessage,
    });

    expect(dispatchSettingsStart).not.toHaveBeenCalled();
    expect(postHostMessage).not.toHaveBeenCalledWith({ type: "settings.snapshot" });
  });
});
