import { AlertCircle, Archive, ArrowLeft, ExternalLink, Info, MoreHorizontal, RotateCcw } from "lucide-react";
import { useRef, useState } from "react";
import type { AgentListedSession } from "@openaide/app-shell-contracts";
import { AgentIcon } from "./AgentIcon";
import { PopupMenu } from "./Popup";
import { SidebarRowActionSlot } from "./SidebarRowParts";
import { nativeSessionTitle, relativeTime } from "./taskSurfaceHelpers";
import { AgentHistoryPreviewDetails, useSidebarTaskPreview } from "./SidebarTaskPreview";

export function SidebarNativeSessionRow({
  archived,
  mutation,
  nativeSessionAgentId,
  nativeSessionAgentName,
  nativeSessionsAdoptingSessionId,
  onArchiveNativeSession,
  onOpenNativeSession,
  onRestoreNativeSession,
  session,
}: {
  archived: boolean;
  mutation?: import("../state/store").NativeSessionMutationState;
  nativeSessionAgentId: string;
  nativeSessionAgentName: string;
  nativeSessionsAdoptingSessionId?: string;
  onArchiveNativeSession: (session: AgentListedSession) => void;
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
      onPointerLeave={() => preview?.leave()}
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
          disabled={adopting || pending}
          onClick={openSession}
          type="button"
        >
          <SessionContent />
        </button>
      )}
      <SidebarRowActionSlot>
        {archived ? (
          <button
            aria-busy={pending || undefined}
            aria-label={`Restore ${title}`}
            className={`task-row-action ${pending ? "pending" : ""}`}
            disabled={pending}
            onClick={() => onRestoreNativeSession(session)}
            title={pending ? "Restoring Native Session" : "Restore Native Session"}
            type="button"
          >
            {pending ? <span className="task-state-spinner" /> : <RotateCcw size={13} />}
          </button>
        ) : <>
          <button
            aria-label={`Open ${title}`}
            className="task-row-action external-session-open-action"
            disabled={adopting || pending}
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
                  disabled={adopting || pending}
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
          {mutation?.state === "failed" ? (
            <span
              aria-label={mutation.error ?? "Native Session archive failed"}
              className="native-session-mutation-error"
              role="img"
              title={mutation.error}
            >
              <AlertCircle size={12} />
              <span>{mutation.action === "restore" ? "Restore failed" : "Archive failed"}</span>
            </span>
          ) : pending ? (
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
