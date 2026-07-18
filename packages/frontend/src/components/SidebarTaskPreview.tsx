import { createContext, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { ExternalLink, FolderRoot, GitBranch } from "lucide-react";
import type { TaskSummary } from "@openaide/app-shell-contracts";
import { relativeTime } from "./taskSurfaceHelpers";

export type SidebarPreviewContent = {
  gitRef?: string;
  projectLabel: string;
  state: string;
  title: string;
  unavailable?: boolean;
  workspaceKind: "location" | "native_session" | "worktree";
  workspaceLabel: string;
};

type Preview = { content: SidebarPreviewContent; left: number; top: number };
type PreviewContext = {
  enter: (content: SidebarPreviewContent, row: HTMLElement, immediate?: boolean) => void;
  leave: () => void;
};

const INITIAL_PREVIEW_DELAY_MS = 1_000;

const Context = createContext<PreviewContext | undefined>(undefined);

export function SidebarTaskPreviewProvider({ children }: { children: ReactNode }) {
  const [preview, setPreview] = useState<Preview>();
  const previewRef = useRef<HTMLDivElement>(null);
  const showTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const hideTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const previewOpen = useRef(false);
  previewOpen.current = preview !== undefined;

  useEffect(() => () => {
    clearTimeout(showTimer.current);
    clearTimeout(hideTimer.current);
  }, []);

  const enter = (content: SidebarPreviewContent, row: HTMLElement, immediate = false) => {
    if (typeof window !== "undefined" && window.matchMedia("(max-width: 760px)").matches) return;
    clearTimeout(showTimer.current);
    clearTimeout(hideTimer.current);
    const open = () => {
      const bounds = row.getBoundingClientRect();
      setPreview({
        content,
        left: Math.min(bounds.right + 8, window.innerWidth - 304),
        top: Math.max(8, Math.min(bounds.top - 7, window.innerHeight - 170)),
      });
    };
    if (immediate || previewOpen.current) open();
    else showTimer.current = setTimeout(open, INITIAL_PREVIEW_DELAY_MS);
  };
  const leave = () => {
    clearTimeout(showTimer.current);
    clearTimeout(hideTimer.current);
    hideTimer.current = setTimeout(() => setPreview(undefined), 140);
  };

  useEffect(() => {
    if (!preview) return;
    const dismiss = (event: PointerEvent) => {
      if (previewRef.current?.contains(event.target as Node)) return;
      setPreview(undefined);
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setPreview(undefined);
    };
    document.addEventListener("pointerdown", dismiss);
    document.addEventListener("keydown", escape);
    return () => { document.removeEventListener("pointerdown", dismiss); document.removeEventListener("keydown", escape); };
  }, [preview]);

  return <Context.Provider value={{ enter, leave }}>
    {children}
    {preview ? <div className="task-preview-popover" onPointerEnter={() => clearTimeout(hideTimer.current)} onPointerLeave={leave} ref={previewRef} role="dialog" style={{ left: preview.left, top: preview.top }}>
      <header><strong>{preview.content.title}</strong><span>{preview.content.state}</span></header>
      <div><FolderRoot size={15} /><span><small>Project</small><strong>{preview.content.projectLabel}</strong></span></div>
      <div className={preview.content.unavailable ? "unavailable" : ""}>
        {preview.content.workspaceKind === "worktree" ? <GitBranch size={15} /> : preview.content.workspaceKind === "native_session" ? <ExternalLink size={15} /> : <FolderRoot size={15} />}
        <span><small>{preview.content.workspaceKind === "worktree" ? "Worktree" : "Location"}</small><strong>{preview.content.workspaceLabel}</strong>{preview.content.gitRef ? <em>{preview.content.gitRef}</em> : null}</span>
      </div>
    </div> : null}
  </Context.Provider>;
}

export function useSidebarTaskPreview() { return useContext(Context); }

/** Maps an adopted Task to the shared compact Sidebar preview. */
export function taskPreviewContent(task: TaskSummary): SidebarPreviewContent {
  return {
    gitRef: task.git_ref,
    projectLabel: task.project_label ?? "Project",
    state: taskState(task),
    title: task.title,
    unavailable: task.workspace_available === false,
    workspaceKind: task.worktree_id ? "worktree" : "location",
    workspaceLabel: task.worktree_name ?? "Project root",
  };
}

function taskState(task: TaskSummary) {
  if (task.status === "active") return "Running";
  if (task.status === "waiting") return "Waiting";
  if (task.status === "failed") return "Failed";
  return relativeTime(task.last_activity);
}
