import { ArrowLeft, ExternalLink, Info, MoreHorizontal } from "lucide-react";
import { useRef, useState } from "react";
import type { AgentListedSession } from "@openaide/app-shell-contracts";
import { AgentIcon } from "./AgentIcon";
import { PopupMenu } from "./Popup";
import { SidebarRowActionSlot } from "./SidebarRowParts";
import { nativeSessionTitle, relativeTime } from "./taskSurfaceHelpers";
import { AgentHistoryPreviewDetails, useSidebarTaskPreview } from "./SidebarTaskPreview";

export function SidebarNativeSessionRow({
  nativeSessionAgentId,
  nativeSessionAgentName,
  nativeSessionsAdoptingSessionId,
  onOpenNativeSession,
  session,
}: {
  nativeSessionAgentId: string;
  nativeSessionAgentName: string;
  nativeSessionsAdoptingSessionId?: string;
  onOpenNativeSession: (session: AgentListedSession) => void;
  session: AgentListedSession;
}) {
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const rowRef = useRef<HTMLDivElement>(null);
  const preview = useSidebarTaskPreview();
  const adopting = nativeSessionsAdoptingSessionId === session.session_id;
  const title = nativeSessionTitle(session);
  const timestamp = session.last_activity ?? session.updated_at;
  const age = timestamp ? relativeTime(timestamp) : "";
  const openSession = () => {
    setDetailsOpen(false);
    setMenuOpen(false);
    preview?.dismiss();
    onOpenNativeSession(session);
  };
  const changeMenuOpen = (open: boolean) => {
    if (open) preview?.dismiss();
    else setDetailsOpen(false);
    setMenuOpen(open);
  };

  return (
    <div
      className="task-row external-session-row"
      onFocus={() => rowRef.current && preview?.enter(previewContent(), rowRef.current, true)}
      onPointerEnter={() => rowRef.current && preview?.enter(previewContent(), rowRef.current)}
      onPointerLeave={() => preview?.leave()}
      ref={rowRef}
      role="listitem"
    >
      <button
        className="task-open"
        disabled={adopting}
        onClick={openSession}
        type="button"
        aria-label={`Open ${title}`}
      >
        <span aria-label={`Agent: ${nativeSessionAgentName}`} className="task-agent-icon" role="img" title={nativeSessionAgentName}>
          <AgentIcon agentId={nativeSessionAgentId} agentName={nativeSessionAgentName} size={12} />
        </span>
        <span className="task-row-body">
          <span className="task-title">{title}</span>
          <span className="task-trailing-meta">
            {adopting ? (
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
      </button>
      <SidebarRowActionSlot>
        <button
          aria-label={`Open ${title}`}
          className="task-row-action external-session-open-action"
          disabled={adopting}
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
                disabled={adopting}
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
            </>}
          </PopupMenu>
        </span>
      </SidebarRowActionSlot>
    </div>
  );

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
