import { AlertCircle, Archive, ArrowLeft, Check, ExternalLink, GitFork, Info, MoreHorizontal, RotateCcw } from "lucide-react";
import { useRef, useState } from "react";
import type { AgentListedSession } from "@openaide/app-shell-contracts";
import { AgentIcon } from "./AgentIcon";
import { PopupMenu } from "./Popup";
import { SidebarRowActionSlot } from "./SidebarRowParts";
import { nativeSessionTitle, relativeTime } from "./taskSurfaceHelpers";
import { AgentHistoryPreviewDetails, useSidebarTaskPreview } from "./SidebarTaskPreview";

export function SidebarNativeSessionRow({
  archived,
  canFork = false,
  mutation,
  nativeSessionAgentId,
  nativeSessionAgentName,
  nativeSessionsAdoptingSessionId,
  onArchiveNativeSession,
  onForkNativeSession,
  onOpenNativeSession,
  onRestoreNativeSession,
  session,
}: {
  archived: boolean;
  canFork?: boolean;
  mutation?: import("../state/store").NativeSessionMutationState;
  nativeSessionAgentId: string;
  nativeSessionAgentName: string;
  nativeSessionsAdoptingSessionId?: string;
  onArchiveNativeSession: (session: AgentListedSession) => void;
  onForkNativeSession?: (session: AgentListedSession) => void;
  onOpenNativeSession: (session: AgentListedSession) => void;
  onRestoreNativeSession: (session: AgentListedSession) => void;
  session: AgentListedSession;
}) {
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const rowRef = useRef<HTMLDivElement>(null);
  const preview = useSidebarTaskPreview();
  const adopting = nativeSessionsAdoptingSessionId === session.session_id;
  const pending = mutation?.state === "pending";
  const forkMutation = mutation?.action === "fork" ? mutation : undefined;
  const forkPending = forkMutation?.state === "pending";
  const lifecyclePending = pending && !forkPending;
  const title = nativeSessionTitle(session);
  const timestamp = session.last_activity ?? session.updated_at;
  const age = timestamp ? relativeTime(timestamp) : "";

  const closeMenu = () => {
    setDetailsOpen(false);
    setMenuOpen(false);
  };
  const openSession = () => {
    if (archived) return;
    closeMenu();
    preview?.dismiss();
    onOpenNativeSession(session);
  };
  const archiveSession = () => {
    closeMenu();
    preview?.dismiss();
    onArchiveNativeSession(session);
  };
  const changeMenuOpen = (open: boolean) => {
    if (open) preview?.dismiss();
    else setDetailsOpen(false);
    setMenuOpen(open);
  };

  return (
    <div
      className="task-row external-session-row"
      data-archived-native-session={archived || undefined}
      data-menu-open={menuOpen || undefined}
      onFocus={() => rowRef.current && preview?.enter(previewContent(), rowRef.current, true)}
      onPointerEnter={() => rowRef.current && preview?.enter(previewContent(), rowRef.current)}
      onPointerLeave={(event) => preview?.leave(event.relatedTarget)}
      ref={rowRef}
      role="listitem"
    >
      {archived ? (
        <div className="task-open" aria-label={`Archived Native Session: ${title}`}>
          <SessionContent />
        </div>
      ) : (
        <button
          aria-label={`Open ${title}`}
          className="task-open"
          disabled={adopting || lifecyclePending}
          onClick={openSession}
          type="button"
        >
          <SessionContent />
        </button>
      )}
      <SidebarRowActionSlot>
        {archived ? (
          <button
            aria-busy={lifecyclePending || undefined}
            aria-label={`Restore ${title}`}
            className={`task-row-action ${lifecyclePending ? "pending" : ""}`}
            disabled={lifecyclePending}
            onClick={() => onRestoreNativeSession(session)}
            title={lifecyclePending ? "Restoring Native Session" : "Restore Native Session"}
            type="button"
          >
            {lifecyclePending ? <span className="task-state-spinner" /> : <RotateCcw size={13} />}
          </button>
        ) : <>
          <button
            aria-label={`Open ${title}`}
            className="task-row-action external-session-open-action"
            disabled={adopting || lifecyclePending}
            onClick={openSession}
            title={adopting ? "Opening task" : "Open task"}
            type="button"
          >
            <ExternalLink size={13} />
          </button>
          <span className="external-session-details-actions">
            <PopupMenu
              className="task-row-menu"
              label={`Task actions for ${title}`}
              onOpenChange={changeMenuOpen}
              open={menuOpen}
              trigger={(triggerProps) => (
                <button
                  {...triggerProps}
                  aria-label={`Task actions for ${title}`}
                  className="task-row-action"
                  disabled={adopting || lifecyclePending}
                  title={adopting ? "Opening task" : menuOpen ? undefined : "Task actions"}
                  type="button"
                >
                  <MoreHorizontal size={14} />
                </button>
              )}
            >
              {detailsOpen ? <>
                <button onClick={() => setDetailsOpen(false)} type="button" role="menuitem"><ArrowLeft size={13} />Task actions</button>
                <div className="task-row-details">
                  <AgentHistoryPreviewDetails content={previewContent()} explainSource={false} />
                </div>
              </> : <>
                <button className="task-row-details-action" onClick={() => setDetailsOpen(true)} type="button" role="menuitem"><Info size={13} />Task details</button>
                <button onClick={openSession} type="button" role="menuitem"><ExternalLink size={13} />Open task</button>
                {canFork && onForkNativeSession ? (
                  <button
                    disabled={forkPending}
                    onClick={() => {
                      closeMenu();
                      preview?.dismiss();
                      onForkNativeSession(session);
                    }}
                    type="button"
                    role="menuitem"
                  >
                    <GitFork size={13} />{forkPending ? "Forking…" : "Fork session"}
                  </button>
                ) : null}
                <button onClick={archiveSession} type="button" role="menuitem"><Archive size={13} />Archive</button>
              </>}
            </PopupMenu>
          </span>
        </>}
      </SidebarRowActionSlot>
    </div>
  );

  function SessionContent() {
    return <>
      <span aria-label={`Agent: ${nativeSessionAgentName}`} className="task-agent-icon" role="img" title={nativeSessionAgentName}>
        <AgentIcon agentId={nativeSessionAgentId} agentName={nativeSessionAgentName} size={12} />
      </span>
      <span className="task-row-body">
        <span className="task-title">{title}</span>
        <span className="task-trailing-meta">
          {forkMutation ? (
            <span
              aria-label={forkMutationLabel(forkMutation)}
              className={forkMutation.state === "failed" || forkMutation.state === "unknown" || forkMutation.error
                ? "native-session-mutation-error"
                : "task-trailing-indicator"}
              role="status"
              title={forkMutation.error}
            >
              {forkMutation.state === "pending" ? <span className="task-state-spinner" />
                : forkMutation.state === "created" && !forkMutation.error ? <Check size={12} />
                  : <AlertCircle size={12} />}
              <span>{forkMutationLabel(forkMutation)}</span>
            </span>
          ) : mutation?.state === "failed" ? (
            <span
              aria-label={mutation.error ?? "Native Session archive failed"}
              className="native-session-mutation-error"
              role="img"
              title={mutation.error}
            >
              <AlertCircle size={12} />
              <span>{mutation.action === "restore" ? "Restore failed" : "Archive failed"}</span>
            </span>
          ) : lifecyclePending ? (
            <span
              aria-label={mutation.action === "restore" ? "Restoring Native Session" : "Archiving Native Session"}
              className="task-trailing-indicator"
              role="img"
            >
              <span className="task-state-spinner" />
            </span>
          ) : archived ? (
            <span className="native-session-archive-label">Native Session</span>
          ) : adopting ? (
            <span aria-label="Opening task" className="task-trailing-indicator" role="img" title="Opening task">
              <span className="task-state-spinner" />
            </span>
          ) : age ? (
            <span className="task-meta-age" title={`Last activity: ${timestamp}`}>
              {age}
            </span>
          ) : null}
        </span>
      </span>
    </>;
  }

  function previewContent() {
    return {
      agentName: nativeSessionAgentName,
      kind: "agent_history" as const,
      state: age,
      title,
      workspaceLabel: session.cwd,
    };
  }
}

function forkMutationLabel(mutation: import("../state/store").NativeSessionMutationState) {
  if (mutation.state === "pending") return "Forking";
  if (mutation.state === "created") return mutation.error ? "Created; cleanup failed" : "Fork created";
  if (mutation.state === "unknown") return "Check sessions";
  return "Fork failed";
}
