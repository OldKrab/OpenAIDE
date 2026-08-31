import { describe, expect, it, vi } from "vitest";
import { createDesktopProjectPicker } from "./desktopProjectPicker";

describe("Desktop Project folder selection", () => {
  it("uses the native picker and returns the App Server-native WSL path after warning", async () => {
    const confirm = vi.fn()
      .mockReturnValueOnce(true)
      .mockReturnValueOnce(false);
    const picker = createDesktopProjectPicker({
      confirm,
      invoke: vi.fn(async () => ({
        label: "project",
        path: "/mnt/c/work/project",
        warning: {
          key: "windows-folder-in-wsl",
          message: "This Windows folder may work better in Windows mode.",
        },
      })),
      openDirectory: vi.fn(async () => "C:\\work\\project"),
    });

    await expect(picker()).resolves.toEqual({ label: "project", path: "/mnt/c/work/project" });
    expect(confirm).toHaveBeenCalledWith("This Windows folder may work better in Windows mode.\n\nContinue with this folder?");
    expect(confirm).toHaveBeenCalledWith("Don't show this cross-environment warning again?");
  });

  it("does not repeat a dismissed cross-environment warning", async () => {
    const confirm = vi.fn(() => true);
    const invoke = vi.fn(async (command: string) => command === "desktop_prepare_project_folder"
      ? {
          label: "project",
          path: "/mnt/c/work/project",
          warning: { key: "windows-folder-in-wsl", message: "Use Windows mode." },
        }
      : undefined);
    const picker = createDesktopProjectPicker({
      confirm,
      invoke,
      openDirectory: vi.fn(async () => "C:\\work\\project"),
    });

    await picker();
    await picker();

    expect(confirm).toHaveBeenCalledTimes(2);
    expect(invoke).toHaveBeenCalledWith("desktop_dismiss_path_warning", { key: "windows-folder-in-wsl" });
  });
});
