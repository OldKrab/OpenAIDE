import { act, create } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  AttachmentListDirectoryResult,
  FileBrowserEntryId,
  FileBrowserRoot,
  FileBrowserRootId,
} from "@openaide/app-server-client";
import { ComposerFileBrowser } from "./ComposerFileBrowser";
import type { TaskFileBrowserCallbacks } from "./appControllerCallbackTypes";

describe("ComposerFileBrowser", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("updates long file listing loads with progressive status copy", () => {
    vi.useFakeTimers();
    const browser = fileBrowserCallbacks();
    browser.listRoots = vi.fn(() => new Promise<FileBrowserRoot[]>(() => undefined));

    let tree: ReturnType<typeof create> | undefined;
    act(() => {
      tree = create(<ComposerFileBrowser browser={browser} onAttached={vi.fn()} />);
    });

    expect(textContent(tree!)).toContain("Loading files");

    act(() => {
      vi.advanceTimersByTime(1200);
    });

    expect(textContent(tree!)).toContain("Still loading workspace files");
    expect(textContent(tree!)).toContain("Waiting for App Server file listing.");
  });

  it("keeps its directory when callback implementations refresh for the same owner", async () => {
    const first = fileBrowserCallbacks("task-1");
    const refreshed = fileBrowserCallbacks("task-1");
    let tree: ReturnType<typeof create> | undefined;

    await act(async () => {
      tree = create(<ComposerFileBrowser browser={first} onAttached={vi.fn()} />);
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(textContent(tree!)).toContain("Workspace");

    await act(async () => {
      tree!.update(<ComposerFileBrowser browser={refreshed} onAttached={vi.fn()} />);
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(first.listRoots).toHaveBeenCalledTimes(1);
    expect(refreshed.listRoots).not.toHaveBeenCalled();
    expect(textContent(tree!)).toContain("Workspace");
  });

  it("ignores a directory response owned by the previous task", async () => {
    let resolveOldDirectory: ((listing: AttachmentListDirectoryResult) => void) | undefined;
    const oldDirectory = new Promise<AttachmentListDirectoryResult>((resolve) => {
      resolveOldDirectory = resolve;
    });
    const first = fileBrowserCallbacks("task-1");
    first.listDirectory = vi.fn()
      .mockResolvedValueOnce(directoryListing("Task one", [{
        entryId: "old-folder" as FileBrowserEntryId,
        kind: "directory",
        label: "Old folder",
        selectable: false,
      }]))
      .mockReturnValueOnce(oldDirectory);
    const second = fileBrowserCallbacks("task-2", "Task two");
    let tree: ReturnType<typeof create> | undefined;

    await act(async () => {
      tree = create(<ComposerFileBrowser browser={first} onAttached={vi.fn()} />);
      await Promise.resolve();
      await Promise.resolve();
    });
    act(() => {
      tree!.root.findByProps({ children: "Old folder" }).parent?.props.onClick();
    });

    await act(async () => {
      tree!.update(<ComposerFileBrowser browser={second} onAttached={vi.fn()} />);
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(textContent(tree!)).toContain("Task two");

    await act(async () => {
      resolveOldDirectory?.(directoryListing("Stale task one"));
      await oldDirectory;
      await Promise.resolve();
    });

    expect(textContent(tree!)).toContain("Task two");
    expect(textContent(tree!)).not.toContain("Stale task one");
  });
});

function textContent(tree: ReturnType<typeof create>) {
  return tree.root
    .findAll((node) => typeof node.type === "string")
    .flatMap((node) => node.children)
    .filter((child): child is string => typeof child === "string")
    .join(" ");
}

function fileBrowserCallbacks(
  ownerKey = "task-1",
  directoryLabel = "Workspace",
): TaskFileBrowserCallbacks & { ownerKey: string } {
  return {
    ownerKey,
    attachEmbedded: vi.fn(async () => undefined),
    attachFileReference: vi.fn(async () => undefined),
    attachPastedImage: vi.fn(async () => undefined),
    listDirectory: vi.fn(async () => directoryListing(directoryLabel)),
    listRoots: vi.fn(async () => [{ label: "Workspace", rootId: "root-1" as FileBrowserRootId }]),
  };
}

function directoryListing(
  label: string,
  entries: AttachmentListDirectoryResult["entries"] = [],
): AttachmentListDirectoryResult {
  return {
    directory: { label, rootId: "root-1" as FileBrowserRootId },
    entries,
  };
}
