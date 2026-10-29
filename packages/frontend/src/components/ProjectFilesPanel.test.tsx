// @vitest-environment jsdom

import { act, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  FILE_VIEWER_CHANGES,
  FILE_VIEWER_DIFF,
  FILE_VIEWER_LIST_DIRECTORY,
  FILE_VIEWER_OPEN,
  type ProjectFilesParams,
} from "@openaide/app-server-client";
import { ProjectFilesPanel } from "./ProjectFilesPanel";
import type { FileNote, FileWorkspace } from "./projectFilesModel";
import { useProjectChanges } from "./useProjectFiles";
import { useTaskFileViewer } from "./useTaskFileViewer";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const home: FileWorkspace = {
  id: "root",
  label: "app",
  kind: "Project root",
  branch: "main",
  path: "/work/app",
  scope: { taskId: "t1" },
  taskWorkspace: true,
};
const other: FileWorkspace = {
  id: "wt-fix",
  label: "fix-login",
  kind: "Worktree",
  branch: "fix/login",
  path: "/work/trees/fix-login",
  scope: { projectId: "p1", worktreeId: "wt-fix" },
  taskWorkspace: false,
};

/** A workspace with one changed file and one untouched file, answered like App Server would. */
function server({ repository = true } = {}) {
  return vi.fn(async (method: string, params: ProjectFilesParams & { path: string }) => {
    if (method === FILE_VIEWER_CHANGES)
      return repository
        ? {
            entries: [{ path: "src/login.ts", name: "login.ts", directory: false, status: "modified", added: 1, removed: 1 }],
            total: 1,
            truncated: false,
            defaultBranch: null,
          }
        : { entries: [], truncated: false, error: "notRepository" };
    if (method === FILE_VIEWER_DIFF)
      return {
        entries: [],
        truncated: false,
        diff: {
          binary: false,
          conflicted: false,
          hunks: [
            {
              heading: "@@ -1,2 +1,2 @@",
              lines: [
                { kind: "context", oldLine: 1, newLine: 1, text: "const a = 1;" },
                { kind: "remove", oldLine: 2, newLine: null, text: "const b = 2;" },
                { kind: "add", oldLine: null, newLine: 2, text: "const b = 3;" },
              ],
            },
          ],
        },
      };
    if (method === FILE_VIEWER_LIST_DIRECTORY)
      return {
        entries: params.path
          ? [{ path: "src/login.ts", name: "login.ts", directory: false }]
          : [
              { path: "src", name: "src", directory: true },
              { path: "notes.txt", name: "notes.txt", directory: false },
            ],
        truncated: false,
      };
    if (method === FILE_VIEWER_OPEN)
      return {
        handle: `handle:${params.path}`,
        displayPath: params.path,
        basename: params.path.split("/").at(-1),
        kind: "source",
        text: "plain one\nplain two",
        truncated: false,
      };
    return {};
  });
}

let container: HTMLElement;
let root: Root;
beforeEach(() => {
  localStorage.clear();
  vi.spyOn(console, "info").mockImplementation(() => undefined);
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});
afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
  vi.restoreAllMocks();
});

function Harness({
  request,
  onSaveNote,
  initial = home,
}: {
  request: ReturnType<typeof server>;
  onSaveNote: (note: FileNote, replaces?: FileNote) => void;
  initial?: FileWorkspace;
}) {
  const connection = { request: request as never };
  const [workspace, setWorkspace] = useState(initial);
  const [notes, setNotes] = useState<FileNote[]>([]);
  const changes = useProjectChanges({ connection, enabled: true, scope: workspace.scope, refresh: 0 });
  const viewer = useTaskFileViewer({ connection, enabled: true, scope: workspace.scope, workspaceRoot: workspace.path });
  return (
    <ProjectFilesPanel
      changes={changes}
      connection={connection}
      notes={notes}
      onClose={() => undefined}
      onRefresh={() => undefined}
      onSaveNote={(note, replaces) => {
        onSaveNote(note, replaces);
        setNotes((old) => [...old.filter((item) => item.id !== note.id), note]);
      }}
      onWorkspace={(id) => setWorkspace(id === other.id ? other : home)}
      refresh={0}
      variant="task"
      viewer={viewer}
      workspace={workspace}
      workspaces={[home, other]}
    />
  );
}

/** What the user can see: a view kept mounted behind the other one does not count. */
const shown = (selector: string) =>
  [...container.querySelectorAll<HTMLElement>(selector)].find((element) => !element.closest("[hidden]"));
const button = (name: string | RegExp) =>
  [...container.querySelectorAll<HTMLElement>("button, [role=treeitem]")].find((element) => {
    if (element.closest("[hidden]")) return false;
    const label = element.getAttribute("aria-label") ?? element.textContent ?? "";
    return typeof name === "string" ? label.trim() === name : name.test(label);
  });
const click = (element?: Element | null) =>
  act(async () => {
    expect(element, "element to click").toBeTruthy();
    (element as HTMLElement).click();
  });
async function type(text: string) {
  const field = container.querySelector<HTMLTextAreaElement>('textarea[aria-label="Comment"]')!;
  const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!;
  await act(async () => {
    setter.call(field, text);
    field.dispatchEvent(new Event("input", { bubbles: true }));
  });
}
const render = (props: Parameters<typeof Harness>[0]) => act(async () => root.render(<Harness {...props} />));

it("keeps Changes and Files as separate views, each with its own reader", async () => {
  const request = server();
  await render({ request, onSaveNote: vi.fn() });

  // Changes is the Task's first view: the changed file with its counts and its diff.
  expect(button(/^Changes/)?.getAttribute("aria-pressed")).toBe("true");
  expect(container.querySelector('[aria-label="Changed files"]')?.textContent).toContain("login.ts+1−1M");
  expect(shown(".file-code-reader")?.getAttribute("data-diff")).toBe("true");
  expect(shown('[role="tablist"]')).toBeUndefined();

  await click(button("Open in Files"));
  expect(button("Files")?.getAttribute("aria-pressed")).toBe("true");
  expect(shown('[role="tab"]')?.textContent).toBe("login.ts");
  expect(shown(".file-code-reader")?.getAttribute("data-diff")).not.toBe("true");
  expect(request).toHaveBeenCalledWith(FILE_VIEWER_OPEN, expect.objectContaining({ taskId: "t1", path: "src/login.ts" }));

  // The open file is changed, so Files links back to its diff; the File Tab stays for the return.
  await click(button("Show diff"));
  expect(button(/^Changes/)?.getAttribute("aria-pressed")).toBe("true");
  expect(shown('[role="tablist"]')).toBeUndefined();
  await click(button("Files"));
  expect(shown('[role="tab"]')?.textContent).toBe("login.ts");
});

it("sends a comment on diff lines with the compared sides, then offers it for editing", async () => {
  const onSaveNote = vi.fn();
  const request = server();
  await render({ request, onSaveNote });
  const lines = container.querySelectorAll<HTMLElement>(".file-code-reader .file-viewer-line");
  await click(lines[1]);
  await click(lines[2]);
  await type("why change it?");
  await click(button("Add comment"));
  expect(onSaveNote).toHaveBeenCalledWith(
    expect.objectContaining({
      key: "root:uncommitted:src/login.ts",
      text: "why change it?",
      quoted: "src/login.ts (diff; HEAD lines 2; working copy lines 2)\n- const b = 2;\n+ const b = 3;",
    }),
    undefined,
  );
  expect(button("Edit comment")?.textContent).toContain("why change it?");
  // A note made on the diff belongs to the diff: the same file in Files starts clean, and the
  // diff still shows its note, without being read again, on the way back.
  await click(button("Open in Files"));
  expect(button("Edit comment")).toBeUndefined();
  const diffReads = request.mock.calls.filter(([method]) => method === FILE_VIEWER_DIFF).length;
  await click(button(/^Changes/));
  expect(button("Edit comment")?.textContent).toContain("why change it?");
  expect(request.mock.calls.filter(([method]) => method === FILE_VIEWER_DIFF)).toHaveLength(diffReads);
});

it("names the folder in a comment made outside the Task Workspace", async () => {
  const onSaveNote = vi.fn();
  const request = server();
  await render({ request, onSaveNote, initial: other });
  expect(request).toHaveBeenCalledWith(FILE_VIEWER_CHANGES, expect.objectContaining({ projectId: "p1", worktreeId: "wt-fix" }), expect.anything());
  await click(shown(".file-code-reader .file-viewer-line"));
  await click(button("Quote only"));
  expect(onSaveNote.mock.calls[0][0].quoted).toMatch(/^\/work\/trees\/fix-login\/src\/login\.ts \(diff; /);
  expect(button("Quoted lines: add a comment or remove")).toBeTruthy();
});

it("offers only Files when the workspace is not a Git repository", async () => {
  await render({ request: server({ repository: false }), onSaveNote: vi.fn() });
  expect(button(/^Changes/)).toBeUndefined();
  expect(shown('[role="tree"]')?.textContent).toContain("notes.txt");
  await click(button("notes.txt"));
  expect(shown('[role="tab"]')?.textContent).toBe("notes.txt");
});

it("remembers how changes are laid out and shown", async () => {
  await render({ request: server(), onSaveNote: vi.fn() });
  await click(button("One file at a time"));
  expect(container.querySelector(".project-files-one")?.textContent).toContain("1 of 1");
  await click(button("Show as list"));
  await act(async () => root.unmount());
  root = createRoot(container);
  await render({ request: server(), onSaveNote: vi.fn() });
  expect(button("One file at a time")?.getAttribute("aria-pressed")).toBe("true");
  expect(button("Show as tree")).toBeTruthy();
});
