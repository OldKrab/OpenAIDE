import { act, create } from "react-test-renderer";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { WorkspaceListDirectoryResult } from "@openaide/app-server-client";
import { NewWorkspacePicker } from "./NewWorkspacePicker";
import type { WorkspaceBrowserCallbacks } from "./appControllerCallbackTypes";

describe("NewWorkspacePicker", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
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
    expect(textContent(tree!)).toContain("Workspace");
  });

  it("ignores a directory response from a superseded navigation owner", async () => {
    let resolveOldDirectory: ((listing: WorkspaceListDirectoryResult) => void) | undefined;
    const oldDirectory = new Promise<WorkspaceListDirectoryResult>((resolve) => {
      resolveOldDirectory = resolve;
    });
    const first = workspaceBrowser("new-task:1");
    first.listDirectory = vi.fn(() => oldDirectory);
    const second = workspaceBrowser("new-task:2", "Other workspace");
    let tree: ReturnType<typeof create> | undefined;

    await act(async () => {
      tree = create(<NewWorkspacePicker browser={first} onSelect={vi.fn()} />);
      await Promise.resolve();
    });
    act(() => {
      tree!.root.findByProps({ children: "Workspace" }).parent?.props.onClick();
    });
    await act(async () => {
      tree!.update(<NewWorkspacePicker browser={second} onSelect={vi.fn()} />);
      await Promise.resolve();
    });
    expect(textContent(tree!)).toContain("Other workspace");

    await act(async () => {
      resolveOldDirectory?.(directoryListing("Stale workspace"));
      await oldDirectory;
      await Promise.resolve();
    });

    expect(textContent(tree!)).toContain("Other workspace");
    expect(textContent(tree!)).not.toContain("Stale workspace");
  });
});

function workspaceBrowser(
  ownerKey: string,
  rootLabel = "Workspace",
): WorkspaceBrowserCallbacks & { ownerKey: string } {
  return {
    ownerKey,
    listDirectory: vi.fn(async (path) => directoryListing(rootLabel, path)),
    listRoots: vi.fn(async () => [{ label: rootLabel, path: `/${rootLabel.toLowerCase().replaceAll(" ", "-")}` }]),
  };
}

function directoryListing(label: string, path = `/${label.toLowerCase().replaceAll(" ", "-")}`): WorkspaceListDirectoryResult {
  return {
    directory: { label, path },
    entries: [],
  };
}

function textContent(tree: ReturnType<typeof create>) {
  return tree.root
    .findAll((node) => typeof node.type === "string")
    .flatMap((node) => node.children)
    .filter((child): child is string => typeof child === "string")
    .join(" ");
}
