import { Circle, CircleAlert, CircleCheck, CircleX, LoaderCircle } from "lucide-react";
import type { TaskStatus } from "@openaide/app-shell-contracts";
import { AgentIcon } from "./AgentIcon";
import { workspaceLabel } from "./taskSurfaceHelpers";

const STATUS_PRESENTATION = {
  active: { label: "Running", Icon: LoaderCircle },
  stopping: { label: "Stopping", Icon: LoaderCircle },
  waiting: { label: "Waiting", Icon: CircleAlert },
  failed: { label: "Failed", Icon: CircleX },
  completed: { label: "Completed", Icon: CircleCheck },
  inactive: { label: "Ready", Icon: Circle },
} satisfies Record<TaskStatus, { label: string; Icon: typeof Circle }>;

export function taskStatusLabel(status: TaskStatus) {
  return STATUS_PRESENTATION[status].label;
}

export function TaskHeader({
  agentId,
  agentName,
  status,
  statusLabel,
  title,
  workspaceRoot,
  showWorkspaceContext = true,
}: {
  agentId: string;
  agentName: string;
  status: TaskStatus;
  statusLabel?: string;
  title: string;
  workspaceRoot: string;
  showWorkspaceContext?: boolean;
}) {
  const statusPresentation = STATUS_PRESENTATION[status];
  const visibleStatusLabel = statusLabel ?? statusPresentation.label;
  const StatusIcon = statusPresentation.Icon;
  const projectLabel = workspaceRoot.trim() ? workspaceLabel(workspaceRoot) : undefined;
  return (
    <header className="task-header">
      <span className="task-header-title">
        <strong title={title}>{title}</strong>
        <span className="task-header-meta">
          <span
            aria-label={`Task status: ${visibleStatusLabel}`}
            className={`task-header-status ${status}`}
            role="status"
          >
            <StatusIcon aria-hidden="true" size={12} />
            {visibleStatusLabel}
          </span>
          <span className="task-header-agent">
            <AgentIcon agentId={agentId} agentName={agentName} size={11} />
            <span>{agentName}</span>
          </span>
        </span>
      </span>
      {showWorkspaceContext && projectLabel ? <span className="task-header-workspace" title={workspaceRoot}>{projectLabel}</span> : null}
    </header>
  );
}
