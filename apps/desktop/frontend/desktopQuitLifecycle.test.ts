import { describe, expect, it, vi } from "vitest";

import { quitDesktop } from "./desktopQuitLifecycle";

describe("desktop quit lifecycle", () => {
  it("detaches the App Server client before closing the session and native app", async () => {
    const events: string[] = [];
    const requestDetach = vi.fn(async () => {
      events.push("detach");
    });
    const closeSession = vi.fn(() => {
      events.push("close-session");
    });
    const beforeExit = vi.fn(() => {
      events.push("terminal-log");
    });
    const exitApp = vi.fn(async () => {
      events.push("exit-app");
    });

    await quitDesktop({ requestDetach, closeSession, beforeExit, exitApp });

    expect(events).toEqual(["detach", "close-session", "terminal-log", "exit-app"]);
  });

  it("still exits after a detach failure", async () => {
    const events: string[] = [];

    await quitDesktop({
      async requestDetach() {
        events.push("detach");
        throw new Error("transport unavailable");
      },
      closeSession() {
        events.push("close-session");
      },
      beforeExit(outcome) {
        events.push(`terminal-log:${outcome}`);
      },
      async exitApp() {
        events.push("exit-app");
      },
    });

    expect(events).toEqual([
      "detach",
      "close-session",
      "terminal-log:detachFailed",
      "exit-app",
    ]);
  });
});
