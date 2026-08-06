import { describe, expect, it, vi } from "vitest";
import { createSystemNotificationSender } from "./systemNotifications";

describe("system notifications", () => {
  it("uses the Linux desktop notification service without a shell", async () => {
    const launch = vi.fn(async () => undefined);
    const notify = createSystemNotificationSender("linux", launch);

    await notify("Task finished: Review the implementation");

    expect(launch).toHaveBeenCalledWith("notify-send", [
      "--app-name=OpenAIDE",
      "OpenAIDE",
      "Task finished: Review the implementation",
    ]);
  });

  it("uses the built-in macOS notification script without interpolating the message", async () => {
    const launch = vi.fn(async () => undefined);
    const notify = createSystemNotificationSender("darwin", launch);

    await notify('Task needs an answer: Review "quotes"');

    expect(launch).toHaveBeenCalledWith("osascript", [
      "-e",
      "on run argv",
      "-e",
      "display notification (item 2 of argv) with title (item 1 of argv)",
      "-e",
      "end run",
      "--",
      "OpenAIDE",
      'Task needs an answer: Review "quotes"',
    ]);
  });

  it("uses VS Code's registered Windows application identity", async () => {
    const launch = vi.fn(async () => undefined);
    const notify = createSystemNotificationSender("win32", launch);
    const message = "Task failed: Review 'quotes' & separators";

    await notify(message);

    expect(launch).toHaveBeenCalledOnce();
    const [executable, args] = launch.mock.calls[0];
    expect(executable).toBe("powershell.exe");
    expect(args.slice(0, 5)).toEqual([
      "-NoLogo",
      "-NoProfile",
      "-NonInteractive",
      "-ExecutionPolicy",
      "Bypass",
    ]);
    expect(args.at(-1)).toContain("Microsoft.VisualStudioCode");
    expect(args.at(-1)).not.toContain(message);
  });
});
