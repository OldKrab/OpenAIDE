import { act, create } from "react-test-renderer";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { FILE_VIEWER_OPEN, FILE_VIEWER_RELEASE, type FileViewerSnapshot } from "@openaide/app-server-client";
import { FileViewerPanel } from "./FileViewerPanel";
import { useTaskFileViewer } from "./useTaskFileViewer";

describe("Task File Viewer", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  });
  it("opens a clicked path through fileViewer/open and shows the File Tab", async () => {
    const snapshot = sourceSnapshot("#!/bin/sh\necho hi\n");
    const request = vi.fn(async (method: string) => {
      if (method === FILE_VIEWER_OPEN) return snapshot;
      return {};
    });
    let tree: ReturnType<typeof create>;
    act(() => {
      tree = create(<FileViewerHarness request={request} />);
    });

    await act(async () => {
      await tree!.root.findByProps({ "aria-label": "Open sample path" }).props.onClick();
    });

    expect(request).toHaveBeenCalledWith(
      FILE_VIEWER_OPEN,
      expect.objectContaining({ path: "deploy/local-web.sh", taskId: "task-1" }),
    );
    expect(tree!.root.findByProps({ "aria-label": "File Viewer" })).toBeTruthy();
    expect(JSON.stringify(tree!.toJSON())).toContain("local-web.sh");
    expect(JSON.stringify(tree!.toJSON())).toContain("echo hi");
  });

  it("shows the Task Panel before the snapshot returns", async () => {
    const snapshot = sourceSnapshot("#!/bin/sh\nready\n");
    let finish: ((value: FileViewerSnapshot) => void) | undefined;
    const request = vi.fn((method: string) => {
      if (method === FILE_VIEWER_OPEN) {
        return new Promise<FileViewerSnapshot>((resolve) => {
          finish = resolve;
        });
      }
      return Promise.resolve({});
    });
    let tree: ReturnType<typeof create>;
    act(() => {
      tree = create(<FileViewerHarness request={request} />);
    });

    act(() => {
      tree!.root.findByProps({ "aria-label": "Open sample path" }).props.onClick();
    });

    expect(tree!.root.findByProps({ "aria-label": "File Viewer" })).toBeTruthy();
    expect(JSON.stringify(tree!.toJSON())).toContain("Opening file");
    expect(JSON.stringify(tree!.toJSON())).not.toContain("ready");

    await act(async () => {
      finish?.(snapshot);
    });

    expect(JSON.stringify(tree!.toJSON())).toContain("ready");
    expect(JSON.stringify(tree!.toJSON())).not.toContain("Opening file");
  });

  it("replaces the pending File Tab when App Server returns a resolved display path", async () => {
    const snapshot: FileViewerSnapshot = {
      handle: "handle-resolved" as FileViewerSnapshot["handle"],
      displayPath: "/home/old/src/OpenAIDE/file.rs",
      basename: "file.rs",
      kind: "error",
      error: "notFound",
      truncated: false,
    };
    const request = vi.fn(async (method: string) => {
      if (method === FILE_VIEWER_OPEN) return snapshot;
      return {};
    });
    let tree: ReturnType<typeof create>;
    act(() => {
      tree = create(<FileViewerHarness openPath="file.rs" request={request} />);
    });

    await act(async () => {
      await tree!.root.findByProps({ "aria-label": "Open sample path" }).props.onClick();
    });

    expect(tree!.root.findAllByProps({ role: "tab" })).toHaveLength(1);
    expect(JSON.stringify(tree!.toJSON())).not.toContain("Opening file");
    expect(JSON.stringify(tree!.toJSON())).toContain("File not found");
    expect(JSON.stringify(tree!.toJSON())).toContain("/home/old/src/OpenAIDE/file.rs");
  });

  it("releases the handle when the last File Tab closes", async () => {
    const snapshot = sourceSnapshot("line\n");
    const request = vi.fn(async (method: string) => {
      if (method === FILE_VIEWER_OPEN) return snapshot;
      return {};
    });
    let tree: ReturnType<typeof create>;
    act(() => {
      tree = create(<FileViewerHarness request={request} />);
    });
    await act(async () => {
      await tree!.root.findByProps({ "aria-label": "Open sample path" }).props.onClick();
    });

    await act(async () => {
      tree!.root.findByProps({ "aria-label": "Close local-web.sh" }).props.onClick();
    });

    expect(request).toHaveBeenCalledWith(FILE_VIEWER_RELEASE, { handle: snapshot.handle });
    expect(tree!.root.findAllByProps({ "aria-label": "File Viewer" })).toHaveLength(0);
  });

  it("quotes a source line into Composer as ordinary quoted text", () => {
    const onQuote = vi.fn();
    let tree: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <FileViewerPanel
          collapsed={false}
          onClose={vi.fn()}
          onOpenFromHandle={vi.fn()}
          onQuote={onQuote}
          onRefresh={vi.fn()}
          onSelect={vi.fn()}
          onSplitRatio={vi.fn()}
          splitRatio={0.45}
          tab={sourceSnapshot("first\nquoted line")}
          tabs={[sourceSnapshot("first\nquoted line")]}
        />,
      );
    });

    act(() => {
      tree!.root.findByProps({ "aria-label": "Quote line 2" }).props.onClick();
    });

    expect(onQuote).toHaveBeenCalledWith("deploy/local-web.sh:2\nquoted line");
  });

  it("highlights rust source from the snapshot language", () => {
    let tree: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <FileViewerPanel
          collapsed={false}
          onClose={vi.fn()}
          onOpenFromHandle={vi.fn()}
          onQuote={vi.fn()}
          onRefresh={vi.fn()}
          onSelect={vi.fn()}
          onSplitRatio={vi.fn()}
          splitRatio={0.45}
          tab={sourceSnapshot("fn main() {}", { language: "rs" })}
          tabs={[sourceSnapshot("fn main() {}", { language: "rs" })]}
        />,
      );
    });

    expect(tree!.root.findAllByType("span").some((node) => (
      node.children.includes("fn") && typeof node.props.className === "string" && node.props.className.length > 0
    ))).toBe(true);
  });
});

function FileViewerHarness({
  openPath = "deploy/local-web.sh",
  request,
}: {
  openPath?: string;
  request: (method: string, params: unknown) => Promise<unknown>;
}) {
  const viewer = useTaskFileViewer({
    connection: { request: request as never },
    enabled: true,
    taskId: "task-1",
  });
  return (
    <div>
      <button aria-label="Open sample path" onClick={() => void viewer.openPath(openPath)} type="button" />
      <FileViewerPanel
        collapsed={viewer.collapsed}
        onClose={viewer.closeTab}
        onOpenFromHandle={viewer.openFromHandle}
        onQuote={vi.fn()}
        onRefresh={(handle) => void viewer.refresh(handle)}
        onSelect={viewer.selectTab}
        onSplitRatio={viewer.setSplitRatio}
        splitRatio={viewer.splitRatio}
        tab={viewer.activeTab}
        tabs={viewer.tabs}
      />
    </div>
  );
}

function sourceSnapshot(text: string, extra: Partial<FileViewerSnapshot> = {}): FileViewerSnapshot {
  return {
    handle: "handle-1" as FileViewerSnapshot["handle"],
    displayPath: "deploy/local-web.sh",
    basename: "local-web.sh",
    kind: "source",
    text,
    truncated: false,
    ...extra,
  };
}
