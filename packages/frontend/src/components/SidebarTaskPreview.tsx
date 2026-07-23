import { createContext, useCallback, useContext, useEffect, useId, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { FolderRoot, GitBranch } from "lucide-react";
import type { TaskSummary } from "@openaide/app-shell-contracts";
import { relativeTime } from "./taskSurfaceHelpers";
import { PopupHoverSurface } from "./Popup";

type PreviewContentBase = {
  state: string;
  title: string;
  workspaceLabel: string;
};

export type SidebarPreviewContent = PreviewContentBase & (
  | {
      gitRef?: string;
      kind: "task";
      projectLabel: string;
      unavailable?: boolean;
      workspaceKind: "location" | "worktree";
    }
  | {
      agentName: string;
      kind: "agent_history";
    }
);

type Preview = { anchor: HTMLElement; content: SidebarPreviewContent };
type PreviewContext = {
  dismiss: () => void;
  enter: (content: SidebarPreviewContent, row: HTMLElement, immediate?: boolean) => void;
  leave: () => void;
};

const INITIAL_PREVIEW_DELAY_MS = 1_000;

const Context = createContext<PreviewContext | undefined>(undefined);

export function SidebarTaskPreviewProvider({ children }: { children: ReactNode }) {
  const [preview, setPreview] = useState<Preview>();
  const [helpOpen, setHelpOpen] = useState(false);
  const helpId = useId();
  const previewRef = useRef<HTMLDivElement>(null);
  const pendingRowRef = useRef<HTMLElement | undefined>(undefined);
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
    if (!immediate && pendingRowRef.current === row) return;
    clearTimeout(showTimer.current);
    clearTimeout(hideTimer.current);
    pendingRowRef.current = row;
    setHelpOpen(false);
    const open = () => {
      pendingRowRef.current = undefined;
      setPreview({
        anchor: row,
        content,
      });
    };
    if (immediate || previewOpen.current) open();
    else showTimer.current = setTimeout(open, INITIAL_PREVIEW_DELAY_MS);
  };
  const leave = () => {
    clearTimeout(showTimer.current);
    clearTimeout(hideTimer.current);
    pendingRowRef.current = undefined;
    hideTimer.current = setTimeout(() => setPreview(undefined), 140);
  };
  const dismiss = () => {
    clearTimeout(showTimer.current);
    clearTimeout(hideTimer.current);
    pendingRowRef.current = undefined;
    setHelpOpen(false);
    setPreview(undefined);
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

  return <Context.Provider value={{ dismiss, enter, leave }}>
    {children}
    {preview ? <PopupHoverSurface anchor={preview.anchor} className="task-preview-popover" onPointerEnter={() => clearTimeout(hideTimer.current)} onPointerLeave={leave} containerRef={previewRef} semanticRole="dialog">
      {preview.content.kind === "task" ? <TaskPreviewDetails content={preview.content} /> : <>
        <header><ScrollablePreviewTitle title={preview.content.title} /><span className="task-preview-state">{preview.content.state}</span></header>
        <section
          className="task-preview-source-wrap"
          onBlur={(event) => {
            if (!event.currentTarget.contains(event.relatedTarget)) setHelpOpen(false);
          }}
          onPointerLeave={() => setHelpOpen(false)}
        >
          <p className="task-preview-source">
            <button
              aria-describedby={helpOpen ? helpId : undefined}
              aria-expanded={helpOpen}
              aria-label={`What loading from ${preview.content.agentName} means`}
              onClick={() => setHelpOpen((open) => !open)}
              onFocus={() => setHelpOpen(true)}
              onPointerEnter={() => setHelpOpen(true)}
              type="button"
            >From {preview.content.agentName}</button>
            <span className="task-preview-source-action">· Open to load</span>
          </p>
          {helpOpen ? <div className="task-preview-explanation" id={helpId} role="tooltip">
            This conversation exists in {preview.content.agentName} but has not been added to OpenAIDE. Opening it creates an OpenAIDE task and loads its message history. After that, it behaves like your other tasks.
          </div> : null}
        </section>
        <div><FolderRoot size={15} /><span><small>Folder</small><strong>{preview.content.workspaceLabel}</strong></span></div>
      </>}
    </PopupHoverSurface> : null}
  </Context.Provider>;
}

export function useSidebarTaskPreview() { return useContext(Context); }

/** Shared Task facts rendered by desktop hover previews and mobile Task details. */
export function TaskPreviewDetails({
  content,
}: {
  content: Extract<SidebarPreviewContent, { kind: "task" }>;
}) {
  return <>
    <header><ScrollablePreviewTitle title={content.title} /><span className="task-preview-state">{content.state}</span></header>
    <div><FolderRoot size={15} /><span><small>Project</small><strong>{content.projectLabel}</strong></span></div>
    <div className={content.unavailable ? "unavailable" : ""}>
      {content.workspaceKind === "worktree" ? <GitBranch size={15} /> : <FolderRoot size={15} />}
      <span><small>{content.workspaceKind === "worktree" ? "Worktree" : "Location"}</small><strong>{content.workspaceLabel}</strong>{content.gitRef ? <em>{content.gitRef}</em> : null}</span>
    </div>
  </>;
}

function ScrollablePreviewTitle({ title }: { title: string }) {
  const titleRef = useRef<HTMLElement>(null);
  const [moreBelow, setMoreBelow] = useState(false);
  const updateOverflow = useCallback(() => {
    const element = titleRef.current;
    if (!element) return;
    setMoreBelow(element.scrollTop + element.clientHeight < element.scrollHeight - 1);
  }, []);

  useLayoutEffect(() => {
    updateOverflow();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(updateOverflow);
    if (titleRef.current) observer.observe(titleRef.current);
    return () => observer.disconnect();
  }, [title, updateOverflow]);

  return (
    <span className="task-preview-title-wrap" data-more-below={String(moreBelow)}>
      <strong className="task-preview-title" onScroll={updateOverflow} ref={titleRef}>{title}</strong>
    </span>
  );
}

/** Maps an adopted Task to the shared compact Sidebar preview. */
export function taskPreviewContent(
  task: TaskSummary,
): Extract<SidebarPreviewContent, { kind: "task" }> {
  return {
    gitRef: task.git_ref,
    kind: "task",
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
