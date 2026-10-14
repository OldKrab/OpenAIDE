import { describe, expect, it } from "vitest";
import {
  buildChangeTree,
  fileWorkspaces,
  noteBlock,
  readFilePreferences,
  recentToolPaths,
  remember,
  removeNoteFromDraft,
  saveNoteInDraft,
  scopeParams,
  workspaceForPaths,
  writeFilePreferences,
  type FileNote,
} from "./projectFilesModel";

const note = (text: string, quoted = "src/a.ts:3\nconst a = 1;"): FileNote => ({
  id: "n",
  key: "root:file:src/a.ts",
  start: 2,
  end: 2,
  text,
  quoted,
});

const repository = {
  worktrees: [
    { worktreeId: "wt-root", name: "app", path: "/work/app", head: { kind: "branch", name: "main" }, forgotten: false, availability: "available" },
    { worktreeId: "wt-fix", name: "fix-login", path: "/work/trees/fix-login", head: { kind: "branch", name: "fix/login" }, forgotten: false, availability: "available" },
    { worktreeId: "wt-gone", name: "gone", path: "/work/trees/gone", head: { kind: "detached" }, forgotten: false, availability: "missing" },
  ],
} as never;
const project = { projectId: "p1", label: "App", workspaceRoot: "/work/app", projectWorktreeId: "wt-root" };

describe("Project files workspaces", () => {
  it("offers the Project root and available worktrees, with the Task Workspace read through the Task", () => {
    const task = { task_id: "t1", project_id: "p1", worktree_id: "wt-fix", workspace_root: "/work/trees/fix-login" };
    const options = fileWorkspaces({ task, project, repository });
    expect(options.map((option) => [option.id, option.kind, option.branch, option.taskWorkspace])).toEqual([
      ["root", "Project root", "main", false],
      ["wt-fix", "Worktree", "fix/login", true],
    ]);
    expect(options[0].scope).toEqual({ projectId: "p1" });
    expect(options[1].scope).toEqual({ taskId: "t1" });
    expect(scopeParams(options[0].scope, { path: "src" })).toMatchObject({ projectId: "p1", path: "src" });
    expect(scopeParams(options[1].scope)).not.toHaveProperty("projectId");
  });

  it("addresses another worktree by Project and worktree when browsing from the Project", () => {
    const options = fileWorkspaces({ project, repository });
    expect(options.map((option) => option.scope)).toEqual([
      { projectId: "p1" },
      { projectId: "p1", worktreeId: "wt-fix" },
    ]);
  });

  it("still shows the Task Workspace when the Project replica is not loaded", () => {
    const task = { task_id: "t1", workspace_root: "/somewhere/app", git_ref: "main" };
    expect(fileWorkspaces({ task })).toEqual([
      expect.objectContaining({ id: "task", label: "app", branch: "main", scope: { taskId: "t1" }, taskWorkspace: true }),
    ]);
  });

  it("suggests the workspace the Agent last touched, and nothing when that is the current one", () => {
    const options = fileWorkspaces({ task: { task_id: "t1", project_id: "p1", workspace_root: "/work/app" }, project, repository });
    const [root, fix] = options;
    expect(workspaceForPaths(options, ["/work/trees/fix-login/src/a.ts", "/work/app/b.ts"], root)).toBe(fix);
    expect(workspaceForPaths(options, ["/work/app/b.ts", "/work/trees/fix-login/src/a.ts"], root)).toBeUndefined();
    expect(workspaceForPaths(options, ["relative/path.ts"], root)).toBeUndefined();
  });

  it("reads recently touched paths from tool steps, newest first", () => {
    const items = [
      { message: { kind: "user" } },
      { message: { kind: "activity", steps: [{ kind: "tool", details: { locations: [{ path: "/a/old.ts" }] } }] } },
      { message: { kind: "activity", steps: [{ kind: "text" }, { kind: "tool", details: { locations: [], input: { cwd: "/a/new" } } }] } },
    ];
    expect(recentToolPaths(items)).toEqual(["/a/new", "/a/old.ts"]);
  });
});

describe("Project files change tree", () => {
  it("joins single-folder chains into one row and lists folders before files", () => {
    const tree = buildChangeTree(["src/deep/only/file.ts", "README.md", "src/deep/only/other.ts", "docs/a.md"]);
    expect(tree.map((node) => node.label)).toEqual(["docs", "src/deep/only", "README.md"]);
    expect(tree[1].children?.map((node) => node.path)).toEqual(["src/deep/only/file.ts", "src/deep/only/other.ts"]);
  });
});

describe("file notes in the message draft", () => {
  it("appends a note, rewrites it in place when edited, and removes it", () => {
    const first = note("why?");
    const draft = saveNoteInDraft("Please look:", first);
    expect(draft).toBe(`Please look:\n${noteBlock(first)}`);
    expect(draft).toContain("> src/a.ts:3");
    expect(draft).toContain("why?");

    const edited = saveNoteInDraft(`${draft}tail`, note("why not?"), first);
    expect(edited).toBe(`Please look:\n${noteBlock(note("why not?"))}tail`);
    expect(removeNoteFromDraft(edited, note("why not?"))).toBe("Please look:\ntail");
  });

  it("appends an edit when the user rewrote the earlier note text by hand", () => {
    const first = note("why?");
    const draft = saveNoteInDraft("", first).replace("why?", "hand edited");
    const edited = saveNoteInDraft(draft, note("again"), first);
    expect(edited.startsWith(draft)).toBe(true);
    expect(edited.endsWith(noteBlock(note("again")))).toBe(true);
  });
});

describe("Project files preferences", () => {
  function storage() {
    const values = new Map<string, string>();
    return {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => void values.set(key, value),
    } as unknown as Storage;
  }

  it("remembers layout choices and keeps the newest value per key", () => {
    const store = storage();
    expect(readFilePreferences(store)).toMatchObject({ layout: "tree", mode: "all" });
    writeFilePreferences((old) => ({ ...old, mode: "file", workspaces: remember(old.workspaces, "t1", "root") }), store);
    writeFilePreferences((old) => ({ ...old, workspaces: remember(old.workspaces, "t1", "wt-fix") }), store);
    expect(readFilePreferences(store)).toMatchObject({ mode: "file", workspaces: [["t1", "wt-fix"]] });
  });

  it("falls back to defaults when stored data is unreadable or storage refuses writes", () => {
    const broken = { getItem: () => "{not json", setItem: () => { throw new Error("full"); } } as unknown as Storage;
    expect(readFilePreferences(broken)).toMatchObject({ layout: "tree", mode: "all", workspaces: [] });
    expect(writeFilePreferences((old) => ({ ...old, layout: "flat" }), broken).layout).toBe("flat");
  });
});
