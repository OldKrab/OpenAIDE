import { useRef, useState } from "react";
import {
  Archive,
  ArrowLeft,
  Check,
  GitBranch,
  Info,
  MoreHorizontal,
  Pencil,
  RotateCcw,
  Undo2,
  X,
} from "lucide-react";
import type { TaskStatus, TaskSummary } from "@openaide/app-shell-contracts";
import { AgentIcon } from "./AgentIcon";
import { PopupMenu } from "./Popup";
import { SidebarRowActionSlot } from "./SidebarRowParts";
import { relativeTime } from "./taskSurfaceHelpers";
import { TaskPreviewDetails, taskPreviewContent, useSidebarTaskPreview } from "./SidebarTaskPreview";

export function SidebarTaskRow({
  activeTaskId,
  onArchiveTask,
  onOpenTask,
  onRestoreTask,
  onSetTaskTitle,
  showArchived,
  task,
}: {
  activeTaskId?: string;
  onArchiveTask: (taskId: string) => void;
  onOpenTask: (taskId: string) => void;
  onRestoreTask: (taskId: string) => void;
  onSetTaskTitle?: (
    taskId: string,
    title: { kind: "user"; value: string } | { kind: "automatic" },
  ) => Promise<void>;
  showArchived: boolean;
  task: TaskSummary;
}) {
  const [menuOpen, setMenuOpen] = useState(false);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [editingTitle, setEditingTitle] = useState(false);
  const [titleDraft, setTitleDraft] = useState(task.title);
  const [titleError, setTitleError] = useState<string>();
  const [titleSaving, setTitleSaving] = useState(false);
  const rowRef = useRef<HTMLDivElement>(null);
  const preview = useSidebarTaskPreview();
  const title = task.title || "Untitled task";
  const actionLabel = showArchived ? "Restore task" : "Archive task";
  const openTask = () => {
    preview?.dismiss();
    onOpenTask(task.task_id);
  };
  const runAction = () => {
    setMenuOpen(false);
    setDetailsOpen(false);
    if (showArchived) {
      onRestoreTask(task.task_id);
    } else {
      onArchiveTask(task.task_id);
    }
  };
  const beginRename = () => {
    setMenuOpen(false);
    setDetailsOpen(false);
    setTitleDraft(title);
    setTitleError(undefined);
    setEditingTitle(true);
  };
  const cancelRename = () => {
    if (titleSaving) return;
    setEditingTitle(false);
    setTitleError(undefined);
  };
  const saveTitle = async () => {
    if (!onSetTaskTitle || titleSaving) return;
    setTitleSaving(true);
    setTitleError(undefined);
    try {
      await onSetTaskTitle(task.task_id, { kind: "user", value: titleDraft });
      setEditingTitle(false);
    } catch (error) {
      setTitleError(error instanceof Error ? error.message : "Unable to rename task.");
    } finally {
      setTitleSaving(false);
    }
  };
  const resetTitle = async () => {
    if (!onSetTaskTitle || titleSaving) return;
    setMenuOpen(false);
    setTitleSaving(true);
    setTitleError(undefined);
    try {
      await onSetTaskTitle(task.task_id, { kind: "automatic" });
    } catch (error) {
      setTitleError(error instanceof Error ? error.message : "Unable to reset task title.");
    } finally {
      setTitleSaving(false);
    }
  };
  const changeMenuOpen = (open: boolean) => {
    if (open) {
      // A task menu owns this row until it closes; discard any preview dwell.
      preview?.dismiss();
    } else {
      setDetailsOpen(false);
    }
    setMenuOpen(open);
  };
  return (
    <div
      className={`task-row task-product-row ${task.task_id === activeTaskId ? "selected" : ""}`}
      data-menu-open={menuOpen || undefined}
      onPointerLeave={() => preview?.leave()}
      onPointerMove={() => !menuOpen && rowRef.current && preview?.enter(taskPreviewContent(task), rowRef.current)}
      ref={rowRef}
      role="listitem"
    >
      {editingTitle ? (
        <form
          className="task-rename-form"
          onSubmit={(event) => {
            event.preventDefault();
            void saveTitle();
          }}
        >
          <input
            aria-label={`Rename ${title}`}
            autoFocus
            disabled={titleSaving}
            maxLength={200}
            onChange={(event) => setTitleDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                event.preventDefault();
                cancelRename();
              }
            }}
            value={titleDraft}
          />
          <button aria-label="Save task title" disabled={titleSaving} type="submit"><Check size={13} /></button>
          <button aria-label="Cancel task rename" disabled={titleSaving} onClick={cancelRename} type="button"><X size={13} /></button>
          {titleError ? <small role="alert">{titleError}</small> : null}
        </form>
      ) : (
        <button
          className="task-open"
          onFocus={() => !menuOpen && rowRef.current && preview?.enter(taskPreviewContent(task), rowRef.current, true)}
          onClick={openTask}
          type="button"
        >
        <span
          aria-label={`Agent: ${task.agent_name}`}
          className="task-agent-icon"
          role="img"
          title={task.agent_name}
        >
          <AgentIcon agentId={task.agent_id} agentName={task.agent_name} size={12} />
        </span>
        <span className="task-row-body">
          <span className="task-title">{title}</span>
          <TaskTrailingMeta
            status={task.status}
            timestamp={task.last_activity}
            unread={task.unread}
            worktreeName={task.worktree_id ? task.worktree_name ?? "Worktree" : undefined}
          />
        </span>
        </button>
      )}
      <SidebarRowActionSlot>
        <PopupMenu
          className="task-row-menu"
          label={`Task actions for ${title}`}
          onOpenChange={changeMenuOpen}
          open={menuOpen}
          trigger={(triggerProps) => (
            <button
              {...triggerProps}
              className="task-row-action"
              title={menuOpen ? undefined : "Task actions"}
              type="button"
              aria-label={`Task actions for ${title}`}
            >
              <MoreHorizontal size={14} />
            </button>
          )}
        >
          {detailsOpen ? <>
            <button onClick={() => setDetailsOpen(false)} type="button" role="menuitem"><ArrowLeft size={13} />Task actions</button>
            <div className="task-row-details">
              <TaskPreviewDetails content={taskPreviewContent(task)} />
            </div>
          </> : <>
            <button className="task-row-details-action" onClick={() => setDetailsOpen(true)} type="button" role="menuitem"><Info size={13} />Task details</button>
            {onSetTaskTitle && !showArchived ? (
              <button onClick={beginRename} type="button" role="menuitem"><Pencil size={13} />Rename task</button>
            ) : null}
            {onSetTaskTitle && !showArchived && task.title_source === "user" ? (
              <button onClick={() => void resetTitle()} type="button" role="menuitem"><Undo2 size={13} />Reset automatic title</button>
            ) : null}
            <button onClick={runAction} type="button" role="menuitem">
              {showArchived ? <RotateCcw size={13} /> : <Archive size={13} />}
              {actionLabel}
            </button>
          </>}
        </PopupMenu>
      </SidebarRowActionSlot>
      {!editingTitle && titleError ? (
        <small className="task-title-error" role="alert">{titleError}</small>
      ) : null}
    </div>
  );
}

function TaskTrailingMeta({
  status,
  timestamp,
  unread,
  worktreeName,
}: {
  status: TaskStatus;
  timestamp?: string;
  unread: boolean;
  worktreeName?: string;
}) {
  return (
    <span className="task-trailing-meta">
      {worktreeName ? <span aria-label={`Worktree: ${worktreeName}`} className="task-worktree-marker" role="img" title={`Worktree: ${worktreeName}`}><GitBranch size={12} /></span> : null}
      <TaskStateOrAge status={status} timestamp={timestamp} unread={unread} />
    </span>
  );
}

function TaskStateOrAge({ status, timestamp, unread }: { status: TaskStatus; timestamp?: string; unread: boolean }) {
  // Runtime state takes the age slot. Active work is live by definition, so stale
  // persisted unread data must never add a second indicator to the spinner.
  if (status === "active" || status === "stopping") {
    const label = status === "stopping" ? "Stopping" : "In progress";
    return (
      <span aria-label={label} className="task-trailing-indicator" role="img" title={label}>
        <span className="task-state-spinner" />
      </span>
    );
  }
  if (status === "waiting") {
    const label = unread ? "Waiting, unread" : "Waiting";
    return (
      <span aria-label={label} className="task-trailing-indicator" role="img" title={label}>
        <span className="task-state-pause" />
        {unread ? <span className="task-state-unread-badge" /> : null}
      </span>
    );
  }
  if (status === "failed") {
    const label = unread ? "Failed, unread" : "Failed";
    return (
      <span aria-label={label} className="task-trailing-indicator" role="img" title={label}>
        <span className="task-state-error">!</span>
        {unread ? <span className="task-state-unread-badge" /> : null}
      </span>
    );
  }
  if (unread) {
    return (
      <span aria-label="Unread" className="task-trailing-indicator" role="img" title="Unread">
        <span className="task-state-unread-dot" />
      </span>
    );
  }
  const age = timestamp ? relativeTime(timestamp) : "";
  return age ? (
    <span className="task-meta-age" title={`Last activity: ${timestamp}`}>
      {age}
    </span>
  ) : null;
}
