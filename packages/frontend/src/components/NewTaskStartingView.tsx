import { LoaderCircle } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { TaskHeader } from "./TaskHeader";

const SLOW_START_DELAY_MS = 5_000;

export function NewTaskStartingView({
  agentIcon,
  agentId,
  agentName,
  composer,
  openingNativeSession,
  workspaceRoot,
}: {
  agentIcon?: import("@openaide/app-shell-contracts").AgentIconId;
  agentId: string;
  agentName: string;
  composer: ReactNode;
  openingNativeSession: boolean;
  workspaceRoot: string;
}) {
  const [showSlowStartHint, setShowSlowStartHint] = useState(false);
  const statusLabel = openingNativeSession ? "Opening task" : "Starting task";

  useEffect(() => {
    const timeout = globalThis.setTimeout(() => setShowSlowStartHint(true), SLOW_START_DELAY_MS);
    return () => globalThis.clearTimeout(timeout);
  }, []);

  return (
    <section className="task-surface new-task-starting-surface" aria-label={statusLabel}>
      <TaskHeader
        agentIcon={agentIcon}
        agentId={agentId}
        agentName={agentName}
        status="active"
        statusLabel={openingNativeSession ? "Opening" : "Starting"}
        title="New task"
        workspaceRoot={workspaceRoot}
      />
      <div className="chat-column new-task-starting-chat">
        <div className="message-list-shell">
          <div className="message-list">
            <div className="new-task-starting-status" role="status" aria-live="polite">
              <LoaderCircle aria-hidden="true" size={14} />
              <span>{statusLabel}…</span>
            </div>
            {showSlowStartHint ? (
              <p className="new-task-slow-start-hint">This is taking longer than usual.</p>
            ) : null}
          </div>
        </div>
        {composer}
      </div>
    </section>
  );
}
