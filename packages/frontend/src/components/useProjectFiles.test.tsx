import { act, create } from "react-test-renderer";
import { beforeEach, expect, it, vi } from "vitest";
import type { ProjectFilesParams, ProjectFilesResult } from "@openaide/app-server-client";
import type { FileScope } from "./projectFilesModel";
import { useProjectChanges } from "./useProjectFiles";

beforeEach(() => {
  (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  vi.spyOn(console, "info").mockImplementation(() => undefined);
});

const result = (extra: Partial<ProjectFilesResult>): ProjectFilesResult =>
  ({ entries: [], truncated: false, ...extra }) as ProjectFilesResult;
const entry = (path: string) => ({ path, name: path, directory: false, status: "modified", added: 2, removed: 1 });

function harness(request: (method: string, params: ProjectFilesParams) => Promise<ProjectFilesResult>) {
  let changes!: ReturnType<typeof useProjectChanges>;
  const onBase = vi.fn();
  function Harness({ scope, refresh }: { scope: FileScope; refresh: number }) {
    changes = useProjectChanges({ connection: { request: request as never }, enabled: true, scope, onBase, refresh });
    return null;
  }
  let tree!: ReturnType<typeof create>;
  return {
    onBase,
    get changes() {
      return changes;
    },
    render: async (scope: FileScope, refresh = 0) =>
      act(async () => {
        if (tree) tree.update(<Harness scope={scope} refresh={refresh} />);
        else tree = create(<Harness scope={scope} refresh={refresh} />);
      }),
  };
}

it("compares with the default branch first and falls back to uncommitted work when there is none", async () => {
  const request = vi.fn(async (_method: string, params: ProjectFilesParams) =>
    params.base === "defaultBranch"
      ? result({ error: "noDefaultBranch" })
      : result({ entries: [entry("a.ts")] as never, total: 1 }),
  );
  const view = harness(request);
  await view.render({ taskId: "t1" });
  expect(request.mock.calls.map(([, params]) => params.base)).toEqual(["defaultBranch", "uncommitted"]);
  expect(view.changes).toMatchObject({ total: 1, base: "uncommitted", defaultBranch: undefined, repository: true });
  expect(view.changes.page.error).toBeUndefined();
  // The fallback is the server's answer, not a choice to remember.
  expect(view.onBase).not.toHaveBeenCalled();
});

it("offers the default branch as a base only when the server names one, and remembers a chosen base", async () => {
  const request = vi.fn(async (_method: string, params: ProjectFilesParams) =>
    result({ entries: [entry("a.ts"), entry("b.ts")] as never, total: 7, defaultBranch: params.base === "defaultBranch" ? "main" : null }),
  );
  const view = harness(request);
  await view.render({ projectId: "p1", worktreeId: "wt" });
  expect(request).toHaveBeenLastCalledWith(expect.anything(), expect.objectContaining({ projectId: "p1", worktreeId: "wt", base: "defaultBranch" }), expect.anything());
  expect(view.changes).toMatchObject({ total: 7, base: "defaultBranch", defaultBranch: "main" });
  await act(async () => view.changes.setBase("uncommitted"));
  expect(view.onBase).toHaveBeenCalledWith("uncommitted");
  expect(view.changes.requestedBase).toBe("uncommitted");
});

it("reports a workspace that is not a repository so only Files is offered", async () => {
  const view = harness(async () => result({ error: "notRepository" }));
  await view.render({ projectId: "p1" });
  expect(view.changes).toMatchObject({ repository: false, total: 0 });
});

it("drops a change list that arrives after another workspace was selected", async () => {
  const pending = new Map<string, (value: ProjectFilesResult) => void>();
  const request = vi.fn(
    (_method: string, params: ProjectFilesParams) =>
      new Promise<ProjectFilesResult>((resolve) => pending.set(params.taskId ?? params.worktreeId ?? "", resolve)),
  );
  const view = harness(request);
  await view.render({ taskId: "t1" });
  await view.render({ projectId: "p1", worktreeId: "wt" });
  await act(async () => pending.get("t1")!(result({ entries: [entry("stale.ts")] as never, total: 1 })));
  expect(view.changes.entries).toEqual([]);
  await act(async () => pending.get("wt")!(result({ entries: [entry("fresh.ts")] as never, total: 1 })));
  expect(view.changes.entries.map((item) => item.path)).toEqual(["fresh.ts"]);
});

it("re-reads on refresh and keeps the previous rows on screen meanwhile", async () => {
  let finish!: (value: ProjectFilesResult) => void;
  let calls = 0;
  const request = vi.fn(() =>
    ++calls === 1
      ? Promise.resolve(result({ entries: [entry("a.ts")] as never, total: 1 }))
      : new Promise<ProjectFilesResult>((resolve) => { finish = resolve; }),
  );
  const view = harness(request as never);
  await view.render({ taskId: "t1" }, 0);
  await view.render({ taskId: "t1" }, 1);
  expect(view.changes.page.loading).toBe(true);
  expect(view.changes.entries.map((item) => item.path)).toEqual(["a.ts"]);
  await act(async () => finish(result({ entries: [], total: 0 })));
  expect(view.changes).toMatchObject({ total: 0, entries: [] });
});
