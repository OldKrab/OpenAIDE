import { useCallback, useEffect, useMemo, useState } from "react";
import type {
  BackendConnection,
  WorktreeRepositorySnapshot,
} from "@openaide/app-server-client";
import {
  ProjectFilesPanel,
  type ProjectFileSearchRequest,
} from "./ProjectFilesPanel";
import type { FileSearchMode } from "./ProjectFilesPalette";
import {
  fileWorkspaces,
  noteBlock,
  type FileNote,
  readFilePreferences,
  remember,
  writeFilePreferences,
} from "./projectFilesModel";
import { useBackNavigation } from "./useBackNavigation";
import { useFileSearchShortcuts } from "./useFileSearchShortcuts";
import { useProjectChanges } from "./useProjectFiles";
import { useTaskFileViewer } from "./useTaskFileViewer";
import "../styles/app/project-files-page.css";

/**
 * Project files opened from a Project rather than a Task: the same panel as a page. There is no
 * Composer here, so comments collect on the page and "Ask in a new Task" starts a Task in that
 * Project with all of them as its message. Unsent comments are page state and end with it.
 */
// TODO: give this page a route of its own. It is local presentation state today, so a reload or
// a shared link returns to the surface underneath; routing needs a shell navigation contract.
export function ProjectFilesPage({
  connection,
  onAsk,
  onClose,
  project,
  refreshWorktrees,
  repository,
}: {
  connection: Pick<BackendConnection, "request">;
  /** Starts a new Task in this Project with `prompt` as its message. */
  onAsk: (prompt: string) => void;
  onClose: () => void;
  project: {
    projectId: string;
    label: string;
    workspaceRoot?: string;
    projectWorktreeId?: string | null;
  };
  refreshWorktrees?: () => Promise<void>;
  repository?: Pick<WorktreeRepositorySnapshot, "worktrees">;
}) {
  const projectId = project.projectId;
  const [selected, setSelected] = useState("root");
  const [refresh, setRefresh] = useState(0);
  const [notes, setNotes] = useState<FileNote[]>([]);
  const [searchRequest, setSearchRequest] =
    useState<ProjectFileSearchRequest>();
  const workspaces = useMemo(
    () => fileWorkspaces({ project, repository }),
    [project, repository],
  );
  const workspace =
    workspaces.find((candidate) => candidate.id === selected) ?? workspaces[0];
  const baseKey = `project:${projectId}:${workspace?.id ?? ""}`;
  const rememberedBase = useMemo(
    () => readFilePreferences().bases.find(([key]) => key === baseKey)?.[1],
    [baseKey],
  );
  const changes = useProjectChanges({
    connection,
    enabled: workspace !== undefined,
    scope: workspace?.scope ?? { projectId },
    remembered: rememberedBase,
    onBase: (base) =>
      writeFilePreferences((old) => ({
        ...old,
        bases: remember(old.bases, baseKey, base),
      })),
    refresh,
  });
  const viewer = useTaskFileViewer({
    connection,
    enabled: workspace !== undefined,
    scope: workspace?.scope ?? { projectId },
    workspaceRoot: workspace?.path,
  });
  const bump = useCallback(
    (trigger: "open" | "request") => {
      console.info(
        `project_files_refresh trigger=${trigger} project_id=${projectId} worktrees=${Boolean(refreshWorktrees)}`,
      );
      setRefresh((value) => value + 1);
      if (!refreshWorktrees) return;
      const startedAt = performance.now();
      const done = (outcome: string) =>
        `project_files_worktrees_refreshed trigger=${trigger} project_id=${projectId} outcome=${outcome} duration_ms=${Math.round(performance.now() - startedAt)}`;
      void refreshWorktrees().then(
        () => console.info(done("success")),
        () => console.warn(done("failure")),
      );
    },
    [projectId, refreshWorktrees],
  );
  useEffect(() => bump("open"), [projectId]);
  useBackNavigation(true, onClose, 90);
  const openSearch = useCallback(
    (mode: FileSearchMode) =>
      setSearchRequest((previous) => ({
        mode,
        sequence: (previous?.sequence ?? 0) + 1,
      })),
    [],
  );
  useFileSearchShortcuts(true, openSearch);
  const ask = () => {
    console.info(
      `project_files_ask_in_new_task project_id=${projectId} notes=${notes.length} commented=${notes.filter((note) => note.text).length}`,
    );
    onAsk(notes.map(noteBlock).join(""));
  };

  if (!workspace)
    return (
      <div className="project-files-page">
        <p className="project-files-note" role="status">
          This Project has no folder to browse.
        </p>
      </div>
    );
  return (
    <div className="project-files-page">
      <ProjectFilesPanel
        changes={changes}
        connection={connection}
        key={projectId}
        headerControl={
          notes.length > 0 && (
            <button type="button" className="project-files-ask" onClick={ask}>
              Ask in a new Task <small>{notes.length}</small>
            </button>
          )
        }
        notes={notes}
        onClose={onClose}
        onRefresh={() => bump("request")}
        onRemoveNote={(note) =>
          setNotes((current) => current.filter((item) => item.id !== note.id))
        }
        onSaveNote={(note, replaces) =>
          setNotes((current) =>
            replaces
              ? current.map((item) => (item.id === replaces.id ? note : item))
              : [...current, note],
          )
        }
        onWorkspace={setSelected}
        refresh={refresh}
        searchRequest={searchRequest}
        variant="project"
        viewer={viewer}
        workspace={workspace}
        workspaces={workspaces}
      />
    </div>
  );
}
