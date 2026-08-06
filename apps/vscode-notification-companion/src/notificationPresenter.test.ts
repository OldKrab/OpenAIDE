import { describe, expect, it, vi } from "vitest";
import { registerNotificationPresenter } from "./notificationPresenter";

describe("local notification presenter", () => {
  it("presents a workspace notification through the local OS adapter", async () => {
    const commands = new Map<string, (...args: unknown[]) => unknown>();
    const notify = vi.fn(async () => undefined);
    registerNotificationPresenter({
      registerCommand(command, handler) {
        commands.set(command, handler);
        return { dispose: vi.fn() };
      },
      notify,
      reportFailure: vi.fn(),
      log: vi.fn(),
    });

    await commands.get("_openaide.notifications.show")?.({
      message: "Task finished: Ship notifications",
    });

    expect(notify).toHaveBeenCalledWith("Task finished: Ship notifications");
  });

  it("reports a failed test notification inside VS Code", async () => {
    const commands = new Map<string, (...args: unknown[]) => unknown>();
    const reportFailure = vi.fn();
    registerNotificationPresenter({
      registerCommand(command, handler) {
        commands.set(command, handler);
        return { dispose: vi.fn() };
      },
      notify: vi.fn(async () => { throw new Error("desktop notifications denied"); }),
      reportFailure,
      log: vi.fn(),
    });

    await commands.get("openaide.testSystemNotification")?.();

    expect(reportFailure).toHaveBeenCalledWith(
      "OpenAIDE could not show a system notification. Check the OpenAIDE Notifications output for details.",
    );
  });

  it("logs and propagates remote delivery failures for the workspace fallback", async () => {
    const commands = new Map<string, (...args: unknown[]) => unknown>();
    const log = vi.fn();
    registerNotificationPresenter({
      registerCommand(command, handler) {
        commands.set(command, handler);
        return { dispose: vi.fn() };
      },
      notify: vi.fn(async () => { throw new Error("desktop notifications denied"); }),
      reportFailure: vi.fn(),
      log,
    });

    await expect(commands.get("_openaide.notifications.show")?.({ message: "Task finished" }))
      .rejects.toThrow("desktop notifications denied");
    expect(log).toHaveBeenCalledWith("local OS notification failed", {
      error: "desktop notifications denied",
    });
  });
});
