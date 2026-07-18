import { ExternalLink } from "lucide-react";
import { useRef } from "react";
import type { AgentListedSession } from "@openaide/app-shell-contracts";
import { AgentIcon } from "./AgentIcon";
import { SidebarRowActionSlot } from "./SidebarRowParts";
import { nativeSessionTitle, relativeTime } from "./taskSurfaceHelpers";
import { useSidebarTaskPreview } from "./SidebarTaskPreview";

export function SidebarNativeSessionRow({
  nativeSessionAgentId,
  nativeSessionAgentName,
  nativeSessionsAdoptingSessionId,
  onOpenNativeSession,
  projectLabel = "Project",
  session,
}: {
  nativeSessionAgentId: string;
  nativeSessionAgentName: string;
  nativeSessionsAdoptingSessionId?: string;
  onOpenNativeSession: (session: AgentListedSession) => void;
  projectLabel?: string;
  session: AgentListedSession;
}) {
  const rowRef = useRef<HTMLDivElement>(null);
  const preview = useSidebarTaskPreview();
  const adopting = nativeSessionsAdoptingSessionId === session.session_id;
  const disabled = nativeSessionsAdoptingSessionId !== undefined || adopting;
  const title = nativeSessionTitle(session);
  const timestamp = session.last_activity ?? session.updated_at;
  const age = timestamp ? relativeTime(timestamp) : "";

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
        disabled={disabled}
        onClick={() => onOpenNativeSession(session)}
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
          className="task-row-action"
          disabled={disabled}
          onClick={() => onOpenNativeSession(session)}
          title={adopting ? "Opening task" : "Open task"}
          type="button"
          aria-label={`Open ${title}`}
        >
          <ExternalLink size={13} />
        </button>
      </SidebarRowActionSlot>
    </div>
  );

  function previewContent() {
    return {
      projectLabel,
      state: age,
      title,
      workspaceKind: "native_session" as const,
      workspaceLabel: session.cwd,
    };
  }
}
