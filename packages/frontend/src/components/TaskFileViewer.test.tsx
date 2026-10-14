import { act, create } from "react-test-renderer";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { FILE_VIEWER_OPEN, FILE_VIEWER_RELEASE, type FileViewerSnapshot } from "@openaide/app-server-client";
import { FileViewerPanel } from "./FileViewerPanel";
import { useTaskFileViewer } from "./useTaskFileViewer";

describe("Task File Viewer", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  });
  it("offers Download independently of preview and retries a failed start", async () => {
    const save = vi.fn().mockResolvedValueOnce("notFound").mockResolvedValue("started");
    const tab = sourceSnapshot("", { kind: "binary", error: "unsupported", basename: "build.vsix" });
    let tree: ReturnType<typeof create>;
    const renderPanel = (downloads?: { save: typeof save }) => (
      <FileViewerPanel downloads={downloads} onClose={vi.fn()}
        onOpenFromHandle={vi.fn()} onRefresh={vi.fn()}
        onSelect={vi.fn()} tab={tab} tabs={[tab]} />
    );
    act(() => { tree = create(renderPanel()); });
    expect(tree!.root.findAllByProps({ "aria-label": "Download file" })).toHaveLength(0);
    act(() => { tree!.update(renderPanel({ save })); });
    await act(async () => { await tree!.root.findByProps({ "aria-label": "Download file" }).props.onClick(); });
    expect(save).toHaveBeenCalledWith(expect.objectContaining({ handle: "handle-1", label: "build.vsix" }), expect.any(AbortSignal));
    expect(JSON.stringify(tree!.toJSON())).toContain("File not found");
    await act(async () => { await tree!.root.findByProps({ "aria-label": "Retry download" }).props.onClick(); });
    expect(save).toHaveBeenCalledTimes(2);
    expect(tree!.root.findAllByProps({ role: "alert" })).toHaveLength(0);
    expect(JSON.stringify(tree!.toJSON())).toContain("Unsupported file");
  });

  it("labels a reduced image preview without showing the text-file truncation message", () => {
    const tab = sourceSnapshot("", { kind: "image", truncated: true, preview: {
      label: "photo.jpg", mediaType: "image/jpeg", dataUrl: "data:image/jpeg;base64,/9j/",
    } });
    let tree: ReturnType<typeof create>;
    act(() => { tree = create(<FileViewerPanel onClose={vi.fn()}
      onOpenFromHandle={vi.fn()} onRefresh={vi.fn()} onSelect={vi.fn()}
      tab={tab} tabs={[tab]} />); });
    expect(JSON.stringify(tree!.toJSON())).toContain("Reduced image preview");
    expect(JSON.stringify(tree!.toJSON())).not.toContain("first 1 MiB");
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

  it("keeps the file capability live across parent renders until the tab closes", async () => {
    const request = vi.fn(async (method: string) => method === FILE_VIEWER_OPEN ? sourceSnapshot("ok") : {});
    let tree: ReturnType<typeof create>;
    act(() => { tree = create(<FileViewerHarness request={request} />); });
    await act(async () => {
      await tree!.root.findByProps({ "aria-label": "Open sample path" }).props.onClick();
    });
    act(() => { tree!.update(<FileViewerHarness request={request} />); });
    expect(request.mock.calls.filter(([method]) => method === FILE_VIEWER_RELEASE)).toHaveLength(0);
    act(() => { tree!.unmount(); });
    expect(request).toHaveBeenCalledWith(FILE_VIEWER_RELEASE, { handle: "handle-1" });
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
      displayPath: "/home/user/src/OpenAIDE/file.rs",
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
    expect(JSON.stringify(tree!.toJSON())).toContain("/home/user/src/OpenAIDE/file.rs");
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

  it("comments on a line range and lets the note be edited or removed", () => {
    const tab = sourceSnapshot("first\nsecond\nthird");
    const notes: { id: string; start: number; end: number; text: string; quoted: string }[] = [];
    const onSaveNote = vi.fn((draft: Omit<(typeof notes)[number], "id">, replaces?: string) => {
      const index = notes.findIndex((note) => note.id === replaces);
      if (index >= 0) notes[index] = { ...draft, id: replaces! };
      else notes.push({ ...draft, id: "note-1" });
    });
    const onRemoveNote = vi.fn();
    const panel = () => (
      <FileViewerPanel
        onClose={vi.fn()}
        onOpenFromHandle={vi.fn()}
        onRefresh={vi.fn()}
        onSelect={vi.fn()}
        readerNotes={(path) => ({ path, notes: [...notes], onSaveNote, onRemoveNote })}
        tab={tab}
        tabs={[tab]}
      />
    );
    let tree: ReturnType<typeof create>;
    act(() => { tree = create(panel()); });
    const line = (index: number) => tree!.root.findByProps({ "data-index": index });
    const submit = () => tree!.root.findByProps({ type: "submit" });
    const send = () => tree!.root.findByProps({ "aria-label": "Comment on the selected lines" }).props.onSubmit({ preventDefault() {} });

    // The first line opens the comment box under it; a second line stretches the range.
    act(() => line(0).props.onClick({ shiftKey: false }));
    expect(submit().children.join("")).toBe("Quote only");
    act(() => line(1).props.onClick({ shiftKey: false }));
    act(() => tree!.root.findByProps({ "aria-label": "Comment" }).props.onChange({ target: { value: "why two?" } }));
    expect(submit().children.join("")).toBe("Add comment");
    act(() => send());
    expect(onSaveNote).toHaveBeenLastCalledWith(
      { start: 0, end: 1, text: "why two?", quoted: "deploy/local-web.sh:1–2\nfirst\nsecond" },
      undefined,
    );

    act(() => tree!.update(panel()));
    act(() => tree!.root.findByProps({ "aria-label": "Edit comment" }).props.onClick());
    expect(submit().children.join("")).toBe("Save");
    act(() => tree!.root.findByProps({ "aria-label": "Comment" }).props.onChange({ target: { value: "why both?" } }));
    act(() => send());
    expect(onSaveNote).toHaveBeenLastCalledWith(expect.objectContaining({ text: "why both?" }), "note-1");

    act(() => tree!.update(panel()));
    act(() => tree!.root.findByProps({ "aria-label": "Edit comment" }).props.onClick());
    act(() => tree!.root.findByProps({ className: "code-compose-remove" }).props.onClick());
    expect(onRemoveNote).toHaveBeenCalledWith("note-1");
  });

  it("shows a bare quote differently from a comment and leaves source read-only without an owner", () => {
    const tab = sourceSnapshot("only line");
    const quoted = { id: "q", start: 0, end: 0, text: "", quoted: "deploy/local-web.sh:1\nonly line" };
    let tree: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <FileViewerPanel onClose={vi.fn()} onOpenFromHandle={vi.fn()} onRefresh={vi.fn()} onSelect={vi.fn()}
          readerNotes={(path) => ({ path, notes: [quoted], onSaveNote: vi.fn() })} tab={tab} tabs={[tab]} />,
      );
    });
    expect(tree!.root.findAllByProps({ "aria-label": "Edit comment" })).toHaveLength(0);
    expect(tree!.root.findByProps({ "aria-label": "Quoted lines: add a comment or remove" })).toBeTruthy();

    act(() => {
      tree!.update(
        <FileViewerPanel onClose={vi.fn()} onOpenFromHandle={vi.fn()} onRefresh={vi.fn()} onSelect={vi.fn()} tab={tab} tabs={[tab]} />,
      );
    });
    act(() => tree!.root.findByProps({ "data-index": 0 }).props.onClick({ shiftKey: false }));
    expect(tree!.root.findAllByProps({ "aria-label": "Comment" })).toHaveLength(0);
  });

  it("highlights rust source from the snapshot language", () => {
    let tree: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <FileViewerPanel
          onClose={vi.fn()}
          onOpenFromHandle={vi.fn()}
          onRefresh={vi.fn()}
          onSelect={vi.fn()}
          tab={sourceSnapshot("fn main() {}", { language: "rs" })}
          tabs={[sourceSnapshot("fn main() {}", { language: "rs" })]}
        />,
      );
    });

    expect(tree!.root.findAllByType("span").some((node) => (
      node.children.includes("fn") && typeof node.props.className === "string" && node.props.className.length > 0
    ))).toBe(true);
  });

  it("shows Markdown as preview and switches that File Tab to raw source", () => {
    const tab = markdownSnapshot("# Heading\n\nUse **bold** text.");
    let tree: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <FileViewerPanel
          onClose={vi.fn()}
          onOpenFromHandle={vi.fn()}
          onRefresh={vi.fn()}
          onSelect={vi.fn()}
          tab={tab}
          tabs={[tab]}
        />,
      );
    });

    const preview = JSON.stringify(tree!.toJSON());
    expect(preview).toContain("Heading");
    expect(preview).toContain("bold");
    expect(preview).not.toContain("**bold**");
    expect(tree!.root.findAllByProps({ "data-index": 0 })).toHaveLength(0);

    act(() => {
      tree!.root.findByProps({ "aria-label": "Show raw Markdown" }).props.onClick();
    });

    const raw = JSON.stringify(tree!.toJSON());
    expect(raw).toContain("**bold**");
    expect(tree!.root.findByProps({ "data-index": 2 })).toBeTruthy();

    act(() => {
      tree!.root.findByProps({ "aria-label": "Show Markdown preview" }).props.onClick();
    });
    expect(JSON.stringify(tree!.toJSON())).not.toContain("**bold**");
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
    scope: { taskId: "task-1" },
  });
  return (
    <div>
      <button aria-label="Open sample path" onClick={() => void viewer.openPath(openPath)} type="button" />
      <FileViewerPanel
        onClose={viewer.closeTab}
        onOpenFromHandle={viewer.openFromHandle}
        onRefresh={(handle) => void viewer.refresh(handle)}
        onSelect={viewer.selectTab}
        tab={viewer.activeTab}
        tabs={viewer.tabs}
      />
    </div>
  );
}

function markdownSnapshot(text: string): FileViewerSnapshot {
  return {
    handle: "handle-md" as FileViewerSnapshot["handle"],
    displayPath: "docs/notes.md",
    basename: "notes.md",
    kind: "markdown",
    language: "md",
    text,
    truncated: false,
  };
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
