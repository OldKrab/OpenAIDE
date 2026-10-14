import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { BackendConnection, WorktreeRepositorySnapshot } from "@openaide/app-server-client";
import type { ProjectFileRequest, ProjectFileSearchRequest } from "./ProjectFilesPanel";
import type { FileSearchMode } from "./ProjectFilesPalette";
import {
  changeTotals,
  fileWorkspaces,
  readFilePreferences,
  remember,
  removeNoteFromDraft,
  saveNoteInDraft,
  workspaceForPaths,
  writeFilePreferences,
  type FileNote,
  type FileWorkspaceTask,
} from "./projectFilesModel";
import { useProjectChanges } from "./useProjectFiles";
import { useTaskFileViewer } from "./useTaskFileViewer";
import { useBackNavigation } from "./useBackNavigation";

/** Reading code needs more room than following Chat does, so the panel starts as the larger side. */
const DEFAULT_SPLIT = 0.58;

type RefreshTrigger = "open" | "turn_end" | "request";

/**
 * Project files as seen from one Task: whether the panel is open, which workspace it shows, the
 * change list behind the header count, and the notes waiting in the message. All of it is
 * ephemeral presentation that resets with the Task; App Server stays the authority for every
 * workspace and result.
 */
export function useTaskProjectFiles({
  canComment,
  changePrompt,
  connection,
  enabled,
  onBeforeComment,
  project,
  prompt,
  recentPaths,
  refreshWorktrees,
  repository,
  task,
  turnOpen,
}: {
  /** False while the Composer cannot take text, which makes every reader read-only. */
  canComment: boolean;
  changePrompt: (prompt: string) => void;
  connection?: Pick<BackendConnection, "request">;
  enabled: boolean;
  /** Runs before a comment reaches the Composer, so a hidden Composer can come back first. */
  onBeforeComment?: () => void;
  project?: { projectId: string; label: string; workspaceRoot?: string; projectWorktreeId?: string | null };
  prompt: string;
  /** Paths the Agent recently worked on, newest first. */
  recentPaths: string[];
  refreshWorktrees?: () => Promise<void>;
  repository?: Pick<WorktreeRepositorySnapshot, "worktrees">;
  task: FileWorkspaceTask;
  turnOpen: boolean;
}) {
  const taskId = task.task_id;
  const [open, setOpen] = useState(false);
  const [opened, setOpened] = useState(false);
  const [maximized, setMaximized] = useState(false);
  const [splitRatio, setSplitRatio] = useState(DEFAULT_SPLIT);
  const [refresh, setRefresh] = useState(0);
  const [notes, setNotes] = useState<FileNote[]>([]);
  const [fileRequest, setFileRequest] = useState<ProjectFileRequest & { taskId: string }>();
  const [searchRequest, setSearchRequest] = useState<ProjectFileSearchRequest>();
  const [dismissed, setDismissed] = useState<string>();
  const [selected, setSelected] = useState<string | undefined>(() => rememberedWorkspace(taskId));
  useEffect(() => {
    setOpen(false);
    setOpened(false);
    setMaximized(false);
    setNotes([]);
    setDismissed(undefined);
    setSelected(rememberedWorkspace(taskId));
  }, [taskId]);

  const workspaces = useMemo(
    () => fileWorkspaces({ task, project, repository }),
    [taskId, task.workspace_root, task.worktree_id, task.worktree_name, task.git_ref, task.project_id, project, repository],
  );
  const home = workspaces.find((candidate) => candidate.taskWorkspace) ?? workspaces[0];
  const workspace = workspaces.find((candidate) => candidate.id === selected) ?? home;
  const baseKey = `${taskId}:${workspace.id}`;
  const rememberedBase = useMemo(
    () => readFilePreferences().bases.find(([key]) => key === baseKey)?.[1],
    [baseKey],
  );
  const changes = useProjectChanges({
    connection,
    enabled,
    scope: workspace.scope,
    remembered: rememberedBase,
    onBase: (base) => writeFilePreferences((old) => ({ ...old, bases: remember(old.bases, baseKey, base) })),
    refresh,
  });
  // Handles live only while the panel is open on one workspace; closing it or selecting another
  // workspace releases them.
  const viewer = useTaskFileViewer({
    connection: enabled && open ? connection : undefined,
    enabled: enabled && open,
    scope: workspace.scope,
    workspaceRoot: workspace.path,
  });

  const worktrees = useRef(refreshWorktrees);
  worktrees.current = refreshWorktrees;
  const bump = useCallback(
    (trigger: RefreshTrigger, withWorktrees: boolean) => {
      // One line per refresh names why the reads that follow were issued.
      console.info(`project_files_refresh trigger=${trigger} task_id=${taskId} worktrees=${withWorktrees}`);
      setRefresh((value) => value + 1);
      if (!withWorktrees || !worktrees.current) return;
      const startedAt = performance.now();
      void worktrees.current().then(
        () => console.info(`project_files_worktrees_refreshed trigger=${trigger} task_id=${taskId} outcome=success duration_ms=${Math.round(performance.now() - startedAt)}`),
        () => console.warn(`project_files_worktrees_refreshed trigger=${trigger} task_id=${taskId} outcome=failure duration_ms=${Math.round(performance.now() - startedAt)}`),
      );
    },
    [taskId],
  );
  const show = useCallback(
    (next: boolean) => {
      setOpen(next);
      if (!next) return setMaximized(false);
      setOpened(true);
      bump("open", true);
    },
    [bump],
  );
  const wasOpen = useRef(turnOpen);
  useEffect(() => {
    if (wasOpen.current && !turnOpen && enabled) bump("turn_end", open);
    wasOpen.current = turnOpen;
  }, [turnOpen]);
  useBackNavigation(open, () => show(false), 90);

  // A sent or cleared message takes its notes with it.
  useEffect(() => {
    if (!prompt && notes.length) setNotes([]);
  }, [prompt]);
  const saveNote = useCallback(
    (note: FileNote, replaces?: FileNote) => {
      onBeforeComment?.();
      changePrompt(saveNoteInDraft(prompt, note, replaces));
      setNotes((items) => (replaces ? items.map((item) => (item.id === replaces.id ? note : item)) : [...items, note]));
    },
    [changePrompt, onBeforeComment, prompt],
  );
  const removeNote = useCallback(
    (note: FileNote) => {
      changePrompt(removeNoteFromDraft(prompt, note));
      setNotes((items) => items.filter((item) => item.id !== note.id));
    },
    [changePrompt, prompt],
  );

  const suggested = useMemo(() => workspaceForPaths(workspaces, recentPaths, home), [workspaces, recentPaths, home]);
  const suggestion = suggested && workspace.id === home?.id && dismissed !== suggested.id ? suggested : undefined;
  const selectWorkspace = (id: string) => {
    setSelected(id);
    writeFilePreferences((old) => ({ ...old, workspaces: remember(old.workspaces, taskId, id) }));
    console.info(`project_files_workspace_selected task_id=${taskId} kind=${id === home?.id ? "taskWorkspace" : id === "root" ? "projectRoot" : "worktree"}`);
  };
  // A closed panel keeps its state but must not re-read every open diff on each turn end.
  const shownRefresh = useRef(refresh);
  if (open) shownRefresh.current = refresh;

  return {
    available: enabled && workspace !== undefined,
    open,
    /** True once the panel has been shown for this Task; it then stays mounted to keep its state. */
    opened,
    maximized,
    splitRatio,
    show,
    count: changes.total,
    totals: changeTotals(changes.entries),
    notes,
    openFile: useCallback(
      (path: string, line?: number) => {
        show(true);
        setFileRequest((previous) => ({ taskId, path, line, sequence: (previous?.sequence ?? 0) + 1 }));
      },
      [show, taskId],
    ),
    openSearch: useCallback(
      (mode: FileSearchMode) => {
        show(true);
        setSearchRequest((previous) => ({ mode, sequence: (previous?.sequence ?? 0) + 1 }));
      },
      [show],
    ),
    panel: workspace && {
      changes,
      commentCount: notes.length,
      fileRequest: fileRequest?.taskId === taskId ? fileRequest : undefined,
      maximized,
      notes,
      onClose: () => show(false),
      onDismissSuggestion: () => setDismissed(suggested?.id),
      onMaximized: setMaximized,
      onRefresh: () => bump("request", true),
      onRemoveNote: canComment ? removeNote : undefined,
      onSaveNote: canComment ? saveNote : undefined,
      onSplitRatio: setSplitRatio,
      onWorkspace: selectWorkspace,
      refresh: shownRefresh.current,
      searchRequest,
      splitRatio,
      suggestion,
      viewer,
      workspace,
      workspaces,
    },
  };
}

function rememberedWorkspace(taskId: string) {
  return readFilePreferences().workspaces.find(([key]) => key === taskId)?.[1];
}
