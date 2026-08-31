import { describe, expect, it, vi } from "vitest";
import type { AppServerSession } from "@openaide/app-server-client";
import type { DesktopUpdateSnapshot } from "../../../packages/frontend/src/services/frontendShell";
import { createDesktopUpdates } from "./desktopUpdates";

describe("Desktop update shell adapter", () => {
  it("keeps the newest full native snapshot", async () => {
    const initial = snapshot(2, "idle");
    let onSnapshot: ((event: { payload: DesktopUpdateSnapshot }) => void) | undefined;
    const invoke = vi.fn(async () => initial);
    const updates = createDesktopUpdates({
      invoke,
      listen: vi.fn(async (_event, handler) => {
        onSnapshot = handler as typeof onSnapshot;
        return () => undefined;
      }),
      openReleaseNotes: vi.fn(),
      reload: vi.fn(),
      session: fakeSession(),
    });
    await vi.waitFor(() => expect(updates.snapshot().revision).toBe(2));

    onSnapshot?.({ payload: snapshot(4, "available") });
    onSnapshot?.({ payload: snapshot(3, "failed") });

    expect(updates.snapshot().revision).toBe(4);
    expect(updates.snapshot().kind).toBe("available");
  });

  it("offers only fixed native commands", async () => {
    const invoke = vi.fn(async (command: string) => snapshot(command === "desktop_update_snapshot" ? 1 : 2, "idle"));
    const updates = createDesktopUpdates({
      invoke,
      listen: vi.fn(async () => () => undefined),
      openReleaseNotes: vi.fn(),
      reload: vi.fn(),
      session: fakeSession(),
    });
    await updates.check();
    await updates.download();
    await updates.cancelDownload();

    expect(invoke.mock.calls.map(([command]) => command)).toEqual([
      "desktop_update_snapshot",
      "desktop_check_for_update",
      "desktop_download_update",
      "desktop_cancel_update_download",
    ]);
    expect(invoke.mock.calls.every((call) => call.length === 1)).toBe(true);
  });

  it("detaches the logical client before installer handoff", async () => {
    const session = fakeSession();
    const invoke = vi.fn(async () => snapshot(2, "applying"));
    const updates = createDesktopUpdates({
      invoke,
      listen: vi.fn(async () => () => undefined),
      openReleaseNotes: vi.fn(),
      reload: vi.fn(),
      session,
    });

    await updates.restartAndUpdate();

    expect(session.request).toHaveBeenCalledTimes(3);
    expect(session.close).toHaveBeenCalledOnce();
    expect(invoke).toHaveBeenLastCalledWith("desktop_install_update");
    expect(session.close.mock.invocationCallOrder[0]).toBeLessThan(
      invoke.mock.invocationCallOrder.at(-1) ?? Number.MAX_SAFE_INTEGER,
    );
  });

  it("continues installer handoff when the final detach response is lost during shutdown", async () => {
    vi.useFakeTimers();
    try {
      const session = fakeSession();
      session.request.mockImplementation(async (method: string) => {
        if (method === "client/updateShutdownPrepare") return { kind: "ready" };
        if (method === "client/detach") return new Promise(() => undefined);
        return {};
      });
      const invoke = vi.fn(async () => snapshot(2, "applying"));
      const updates = createDesktopUpdates({
        invoke,
        listen: vi.fn(async () => () => undefined),
        openReleaseNotes: vi.fn(),
        reload: vi.fn(),
        session,
      });

      const handoff = updates.restartAndUpdate();
      await vi.runAllTimersAsync();

      await expect(Promise.race([
        handoff,
        Promise.resolve("stalled" as const),
      ])).resolves.toBe("started");
      expect(session.close).toHaveBeenCalledOnce();
      expect(invoke).toHaveBeenCalledWith("desktop_install_update");
    } finally {
      vi.useRealTimers();
    }
  });

  it("does not detach when active work requires confirmation", async () => {
    const session = fakeSession();
    session.request.mockImplementationOnce(async () => ({
      kind: "blocked",
      reason: "activeWork",
    }));
    const invoke = vi.fn(async () => snapshot(2, "readyToUpdate"));
    const updates = createDesktopUpdates({
      invoke,
      listen: vi.fn(async () => () => undefined),
      openReleaseNotes: vi.fn(),
      reload: vi.fn(),
      session,
    });

    await expect(updates.restartAndUpdate()).resolves.toBe("activeWork");

    expect(session.close).not.toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalledWith("desktop_install_update");
  });
});

function snapshot(
  revision: number,
  kind: DesktopUpdateSnapshot["kind"],
): DesktopUpdateSnapshot {
  return {
    revision,
    installedVersion: "1.0.0",
    kind,
    offer: kind === "available"
      ? { version: "1.1.0", notes: "Fixes", sizeBytes: 10 }
      : undefined,
  };
}

function fakeSession() {
  return {
    request: vi.fn(async (method: string) => method === "client/updateShutdownPrepare"
      ? { kind: "ready" }
      : {}),
    close: vi.fn(async () => undefined),
  } as unknown as AppServerSession & {
    request: ReturnType<typeof vi.fn>;
    close: ReturnType<typeof vi.fn>;
  };
}
