import { act, create } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { FileBrowserRoot, FileBrowserRootId } from "@openaide/app-server-client";
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
});

function textContent(tree: ReturnType<typeof create>) {
  return tree.root
    .findAll((node) => typeof node.type === "string")
    .flatMap((node) => node.children)
    .filter((child): child is string => typeof child === "string")
    .join(" ");
}

function fileBrowserCallbacks(): TaskFileBrowserCallbacks {
  return {
    attachEmbedded: vi.fn(async () => undefined),
    attachFileReference: vi.fn(async () => undefined),
    attachPastedImage: vi.fn(async () => undefined),
    listDirectory: vi.fn(async () => ({ directory: { label: "Workspace", rootId: "root-1" as FileBrowserRootId }, entries: [] })),
    listRoots: vi.fn(async () => [{ label: "Workspace", rootId: "root-1" as FileBrowserRootId }]),
  };
}
