import type {
  ProjectChangeBase,
  ProjectFileEntry,
  ProjectFilesParams,
  ProjectId,
  TaskId,
  WorktreeId,
  WorktreeRepositorySnapshot,
} from "@openaide/app-server-client";
import { appendQuoteToDraft } from "./quoteSelection";

/** Identity App Server resolves to one workspace root. Exactly one of Task and Project is set. */
export type FileScope = { taskId?: string; projectId?: string; worktreeId?: string };

/** A selectable workspace: the Project root or one worktree of its repository. */
export type FileWorkspace = {
  id: string;
  label: string;
  kind: "Project root" | "Worktree" | "Task Workspace";
  branch?: string;
  /** Folder shown in comment paths made outside the Task Workspace. Never logged. */
  path: string;
  scope: FileScope;
  taskWorkspace: boolean;
};

export type FileWorkspaceTask = {
  task_id: string;
  project_id?: string | null;
  worktree_id?: string | null;
  workspace_root: string;
  worktree_name?: string | null;
  git_ref?: string | null;
};

export function scopeKey(scope: FileScope) {
  return `${scope.taskId ?? ""}|${scope.projectId ?? ""}|${scope.worktreeId ?? ""}`;
}

export function scopeParams(scope: FileScope, extra: Partial<ProjectFilesParams> = {}): ProjectFilesParams {
  return {
    ...(scope.taskId ? { taskId: scope.taskId as TaskId } : {}),
    ...(scope.projectId ? { projectId: scope.projectId as ProjectId } : {}),
    ...(scope.worktreeId ? { worktreeId: scope.worktreeId as WorktreeId } : {}),
    path: "",
    query: "",
    content: false,
    caseSensitive: false,
    includeIgnored: false,
    cursor: 0,
    ...extra,
  };
}

const folderName = (path: string) => path.split(/[/\\]/).filter(Boolean).at(-1) ?? path;

/**
 * App Server owns which worktrees belong to a Project and rejects any other; this only orders
 * the choices it already published. The Task Workspace keeps the Task as its authority so the
 * default view never depends on the Project replica being loaded.
 */
export function fileWorkspaces({
  task,
  project,
  repository,
}: {
  task?: FileWorkspaceTask;
  project?: { projectId: string; label: string; workspaceRoot?: string; projectWorktreeId?: string | null };
  repository?: Pick<WorktreeRepositorySnapshot, "worktrees">;
}): FileWorkspace[] {
  const worktrees = (repository?.worktrees ?? []).filter(
    (worktree) => !worktree.forgotten && worktree.availability === "available",
  );
  const branchOf = (worktreeId?: string | null) => {
    const head = worktrees.find((worktree) => worktree.worktreeId === worktreeId)?.head;
    return head?.kind === "branch" ? head.name : head ? "detached" : undefined;
  };
  const options: FileWorkspace[] = [];
  if (project?.workspaceRoot) {
    const taskIsRoot = Boolean(task && !task.worktree_id && task.project_id === project.projectId);
    options.push({
      id: "root",
      label: folderName(project.workspaceRoot) || project.label,
      kind: "Project root",
      branch: branchOf(project.projectWorktreeId) ?? (taskIsRoot ? (task?.git_ref ?? undefined) : undefined),
      path: project.workspaceRoot,
      scope: taskIsRoot ? { taskId: task!.task_id } : { projectId: project.projectId },
      taskWorkspace: taskIsRoot,
    });
    for (const worktree of worktrees) {
      if (worktree.worktreeId === project.projectWorktreeId) continue;
      const own = task?.worktree_id === worktree.worktreeId;
      options.push({
        id: worktree.worktreeId,
        label: worktree.name,
        kind: "Worktree",
        branch: branchOf(worktree.worktreeId),
        path: worktree.path,
        scope: own ? { taskId: task!.task_id } : { projectId: project.projectId, worktreeId: worktree.worktreeId },
        taskWorkspace: own,
      });
    }
  }
  if (task && !options.some((option) => option.taskWorkspace)) {
    options.unshift({
      id: "task",
      label: task.worktree_name ?? folderName(task.workspace_root),
      kind: task.worktree_id ? "Worktree" : "Task Workspace",
      branch: task.git_ref ?? undefined,
      path: task.workspace_root,
      scope: { taskId: task.task_id },
      taskWorkspace: true,
    });
  }
  return options;
}

/** The workspace other than `current` that most recently touched paths belong to, if any. */
export function workspaceForPaths(workspaces: FileWorkspace[], paths: string[], current?: FileWorkspace) {
  const normalize = (value: string) => value.replaceAll("\\", "/").replace(/\/+$/, "");
  const roots = workspaces
    .map((workspace) => ({ workspace, root: normalize(workspace.path) }))
    .sort((a, b) => b.root.length - a.root.length);
  for (const path of paths) {
    const value = normalize(path);
    const owner = roots.find(({ root }) => value === root || value.startsWith(`${root}/`))?.workspace;
    if (owner) return owner.id === current?.id ? undefined : owner;
  }
  return undefined;
}

export type ChangeTreeNode = { label: string; path: string; children?: ChangeTreeNode[] };

/** Folders with a single folder child share one row, so deep paths do not cost a row per segment. */
export function buildChangeTree(paths: string[]): ChangeTreeNode[] {
  const root: ChangeTreeNode = { label: "", path: "", children: [] };
  for (const path of [...paths].sort()) {
    let node = root;
    const parts = path.split("/");
    parts.forEach((part, index) => {
      const here = parts.slice(0, index + 1).join("/");
      let next = node.children!.find((child) => child.path === here);
      if (!next) {
        next = { label: part, path: here, children: index < parts.length - 1 ? [] : undefined };
        node.children!.push(next);
      }
      node = next;
    });
  }
  const compact = (node: ChangeTreeNode): ChangeTreeNode => {
    let current = node;
    while (current.children?.length === 1 && current.children[0].children && current.path !== "")
      current = { ...current.children[0], label: `${current.label}/${current.children[0].label}` };
    return {
      ...current,
      children: current.children
        ?.map(compact)
        .sort((a, b) => Number(Boolean(b.children)) - Number(Boolean(a.children)) || a.label.localeCompare(b.label)),
    };
  };
  return compact(root).children ?? [];
}

export function changeStatusLetter(status?: string | null) {
  if (status === "untracked" || status === "added") return "A";
  if (status === "deleted") return "D";
  if (status === "renamed") return "R";
  if (status === "conflicted") return "!";
  return "M";
}

export function changeTotals(entries: Pick<ProjectFileEntry, "added" | "removed">[]) {
  return entries.reduce<{ added: number; removed: number }>(
    (sum, entry) => ({ added: sum.added + (entry.added ?? 0), removed: sum.removed + (entry.removed ?? 0) }),
    { added: 0, removed: 0 },
  );
}

/** A comment or bare quote made on file lines and not yet sent. Frontend state only. */
export type FileNote = {
  id: string;
  /** Workspace, view and path the note belongs to; a reader shows only its own notes. */
  key: string;
  start: number;
  end: number;
  text: string;
  /** Reference line and code exactly as quoted into the message. */
  quoted: string;
};

/** The text one note contributes to the message: its File Quote, then the comment. */
export function noteBlock(note: Pick<FileNote, "quoted" | "text">) {
  return `${appendQuoteToDraft("", note.quoted)}${note.text ? `${note.text}\n\n` : "\n"}`;
}

/**
 * Writes a note into the message draft. An edit rewrites the note's earlier text in place; when
 * the user has changed that text by hand it can no longer be located, so the edit is appended.
 */
export function saveNoteInDraft(draft: string, next: FileNote, old?: FileNote) {
  if (old && draft.includes(noteBlock(old))) return draft.replace(noteBlock(old), noteBlock(next));
  return `${draft}${draft && !draft.endsWith("\n") ? "\n" : ""}${noteBlock(next)}`;
}

export function removeNoteFromDraft(draft: string, old: FileNote) {
  return draft.replace(noteBlock(old), "");
}

export type FileChangesLayout = "tree" | "flat";
export type FileChangesMode = "all" | "file";
type Preferences = {
  layout: FileChangesLayout;
  mode: FileChangesMode;
  /** Selected workspace id per Task, newest last. */
  workspaces: [string, string][];
  /** Comparison base per Task and workspace, newest last. */
  bases: [string, ProjectChangeBase][];
};
const PREFERENCES_KEY = "openaide.projectFiles.v1";
const REMEMBERED = 60;
const defaults: Preferences = { layout: "tree", mode: "all", workspaces: [], bases: [] };

/** Layout choices are remembered per device; they are presentation, not product state. */
export function readFilePreferences(storage = safeStorage()): Preferences {
  try {
    const value = JSON.parse(storage?.getItem(PREFERENCES_KEY) ?? "null") as Partial<Preferences> | null;
    return {
      layout: value?.layout === "flat" ? "flat" : "tree",
      mode: value?.mode === "file" ? "file" : "all",
      workspaces: Array.isArray(value?.workspaces) ? value.workspaces : [],
      bases: Array.isArray(value?.bases) ? value.bases : [],
    };
  } catch {
    return defaults;
  }
}

export function writeFilePreferences(update: (old: Preferences) => Preferences, storage = safeStorage()) {
  const next = update(readFilePreferences(storage));
  try {
    storage?.setItem(
      PREFERENCES_KEY,
      JSON.stringify({
        ...next,
        workspaces: next.workspaces.slice(-REMEMBERED),
        bases: next.bases.slice(-REMEMBERED),
      }),
    );
  } catch {
    // Storage can be full or disabled; the choice then lasts for this page only.
  }
  return next;
}

export function remember<T>(entries: [string, T][], key: string, value: T): [string, T][] {
  return [...entries.filter(([existing]) => existing !== key), [key, value]];
}

function safeStorage() {
  try {
    return typeof localStorage === "undefined" ? undefined : localStorage;
  } catch {
    return undefined;
  }
}

type ToolPathSource = {
  message: {
    kind: string;
    steps?: {
      kind: string;
      presentation?: { actions: { subjects?: string[]; scopes?: string[] }[] };
      details?: { locations: { path: string }[]; input?: { cwd?: string; path?: string } };
    }[];
  };
};

/**
 * Paths the Agent most recently worked on, newest first, read from tool steps already in Chat.
 * It is a hint for offering another workspace: shell commands that never report a location are
 * invisible to it, which is why the offer is never acted on automatically.
 */
export function recentToolPaths(items: ToolPathSource[], limit = 30) {
  const paths: string[] = [];
  for (const item of items.slice(-limit).reverse()) {
    if (item.message.kind !== "activity") continue;
    for (const step of [...(item.message.steps ?? [])].reverse()) {
      if (step.kind !== "tool") continue;
      for (const location of step.details?.locations ?? []) paths.push(location.path);
      if (step.details?.input?.path) paths.push(step.details.input.path);
      if (step.details?.input?.cwd) paths.push(step.details.input.cwd);
      for (const action of step.presentation?.actions ?? []) paths.push(...(action.subjects ?? []), ...(action.scopes ?? []));
    }
  }
  return paths;
}
