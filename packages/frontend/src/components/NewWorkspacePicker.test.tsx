import { act, create } from "react-test-renderer";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { WorkspaceListDirectoryResult } from "@openaide/app-server-client";
import { NewWorkspacePicker, preferredWorkspaceStart } from "./NewWorkspacePicker";
import type { WorkspaceBrowserCallbacks } from "./appControllerCallbackTypes";

describe("NewWorkspacePicker", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  });

  it("opens in the home folder and prefills the Project name", async () => {
    const browser = workspaceBrowser("new-task:1");
    let tree: ReturnType<typeof create> | undefined;

    await act(async () => {
      tree = create(<NewWorkspacePicker browser={browser} onSelect={vi.fn()} />);
      await Promise.resolve();
    });

    expect(browser.listDirectory).toHaveBeenCalledWith("/home/old");
    expect(textContent(tree!)).toContain("src");
    expect(textContent(tree!)).toContain("old");
    expect(textContent(tree!)).toContain("Add this folder");
    expect(textContent(tree!)).not.toContain("Use this folder");
    expect(textContent(tree!)).not.toContain("Browse folders");
  });

  it("adds the current folder from the footer", async () => {
    const onSelect = vi.fn();
    const browser = workspaceBrowser("new-task:1");
    let tree: ReturnType<typeof create> | undefined;

    await act(async () => {
      tree = create(<NewWorkspacePicker browser={browser} onSelect={onSelect} />);
      await Promise.resolve();
    });
    act(() => {
      tree!.root.findByProps({ children: "Add this folder" }).props.onClick();
    });

    expect(onSelect).toHaveBeenCalledWith({ path: "/home/old", label: "old" });
  });

  it("keeps the current folders visible while the next directory loads", async () => {
    let resolveDirectory: ((listing: WorkspaceListDirectoryResult) => void) | undefined;
    const browser = workspaceBrowser("new-task:1");
    browser.listDirectory = vi.fn(async (path) => {
      if (path === "/home/old/src") {
        return new Promise<WorkspaceListDirectoryResult>((resolve) => {
          resolveDirectory = resolve;
        });
      }
      return directoryListing(path);
    });
    let tree: ReturnType<typeof create> | undefined;

    await act(async () => {
      tree = create(<NewWorkspacePicker browser={browser} onSelect={vi.fn()} />);
      await Promise.resolve();
    });
    act(() => {
      tree!.root.findByProps({ children: "src" }).parent?.props.onClick();
    });

    expect(textContent(tree!)).toContain("src");
    expect(textContent(tree!)).not.toContain("Loading folders");

    await act(async () => {
      resolveDirectory?.(directoryListing("/home/old/src"));
      await Promise.resolve();
    });
    expect(textContent(tree!)).toContain("OpenAIDE");
  });

  it("does not reload roots when callbacks refresh for the same navigation owner", async () => {
    const first = workspaceBrowser("new-task:1");
    const refreshed = workspaceBrowser("new-task:1");
    let tree: ReturnType<typeof create> | undefined;

    await act(async () => {
      tree = create(<NewWorkspacePicker browser={first} onSelect={vi.fn()} />);
      await Promise.resolve();
    });
    await act(async () => {
      tree!.update(<NewWorkspacePicker browser={refreshed} onSelect={vi.fn()} />);
      await Promise.resolve();
    });

    expect(first.listRoots).toHaveBeenCalledTimes(1);
    expect(refreshed.listRoots).not.toHaveBeenCalled();
    expect(textContent(tree!)).toContain("old");
  });

  it("ignores a directory response from a superseded navigation owner", async () => {
    let resolveOldDirectory: ((listing: WorkspaceListDirectoryResult) => void) | undefined;
    const oldDirectory = new Promise<WorkspaceListDirectoryResult>((resolve) => {
      resolveOldDirectory = resolve;
    });
    const first = workspaceBrowser("new-task:1");
    first.listDirectory = vi.fn(() => oldDirectory);
    const second = workspaceBrowser("new-task:2");
    second.listRoots = vi.fn(async () => [{ label: "other", path: "/home/other" }]);
    let tree: ReturnType<typeof create> | undefined;

    await act(async () => {
      tree = create(<NewWorkspacePicker browser={first} onSelect={vi.fn()} />);
      await Promise.resolve();
    });
    await act(async () => {
      tree!.update(<NewWorkspacePicker browser={second} onSelect={vi.fn()} />);
      await Promise.resolve();
    });
    expect(textContent(tree!)).toContain("other");

    await act(async () => {
      resolveOldDirectory?.(directoryListing("/home/old"));
      await oldDirectory;
      await Promise.resolve();
    });

    expect(textContent(tree!)).toContain("other");
    expect(textContent(tree!)).not.toContain("Desktop");
  });
});

describe("preferredWorkspaceStart", () => {
  it("prefers the home folder over cwd and the filesystem root", () => {
    expect(preferredWorkspaceStart([
      { label: "OpenAIDE", path: "/home/old/src/OpenAIDE" },
      { label: "old", path: "/home/old" },
      { label: "/", path: "/" },
    ])).toBe("/home/old");
  });
});

function workspaceBrowser(ownerKey: string): WorkspaceBrowserCallbacks {
  return {
    ownerKey,
    listDirectory: vi.fn(async (path) => directoryListing(path)),
    listRoots: vi.fn(async () => [
      { label: "OpenAIDE", path: "/home/old/src/OpenAIDE" },
      { label: "old", path: "/home/old" },
      { label: "/", path: "/" },
    ]),
  };
}

function directoryListing(path: string): WorkspaceListDirectoryResult {
  const entries = {
    "/home/old": [
      { label: "Desktop", path: "/home/old/Desktop" },
      { label: "src", path: "/home/old/src" },
    ],
    "/home/old/src": [
      { label: "OpenAIDE", path: "/home/old/src/OpenAIDE" },
    ],
    "/home/other": [],
  }[path] ?? [];
  const label = path === "/" ? "/" : path.split("/").filter(Boolean).at(-1) ?? path;
  const parentPath = path === "/" ? null : path.split("/").slice(0, -1).join("/") || "/";
  return {
    directory: { label, path, parentPath },
    entries,
  };
}

function textContent(tree: ReturnType<typeof create>) {
  return tree.root
    .findAll((node) => typeof node.type === "string")
    .flatMap((node) => node.children)
    .filter((child): child is string => typeof child === "string")
    .join(" ");
}
