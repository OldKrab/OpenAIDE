import { Circle, CircleAlert, CircleCheck, CircleX, FolderRoot, GitBranch, LoaderCircle } from "lucide-react";
import type { TaskPermissionPolicy, TaskStatus } from "@openaide/app-shell-contracts";
import { AgentIcon } from "./AgentIcon";
import { TaskPermissionPolicyControl } from "./TaskPermissionPolicyControl";
import { workspaceLabel } from "./taskSurfaceHelpers";
import type { DesktopWindowCapability } from "../services/frontendShell";
import { desktopDragRegionProps } from "./DesktopTitleBar";
import type { ReactNode } from "react";

const STATUS_PRESENTATION = {
  active: { label: "Running", Icon: LoaderCircle },
  stopping: { label: "Stopping", Icon: LoaderCircle },
  waiting: { label: "Waiting", Icon: CircleAlert },
  failed: { label: "Failed", Icon: CircleX },
  completed: { label: "Completed", Icon: CircleCheck },
  inactive: { label: "Idle", Icon: Circle },
} satisfies Record<TaskStatus, { label: string; Icon: typeof Circle }>;

export function taskStatusLabel(status: TaskStatus) {
  return STATUS_PRESENTATION[status].label;
}

export function TaskHeader({
  agentId,
  agentName,
  desktopWindow,
  status,
  statusLabel,
  title,
  permissionPolicy,
  onPermissionPolicyChange,
  permissionPolicyDisabled = false,
  permissionPolicyDisabledReason,
  workspaceRoot,
  showWorkspaceContext = true,
  worktreeName,
  gitRef,
  agentNavigation,
}: {
  agentId: string;
  agentName: string;
  desktopWindow?: DesktopWindowCapability;
  status: TaskStatus;
  statusLabel?: string;
  title: string;
  permissionPolicy?: TaskPermissionPolicy;
  onPermissionPolicyChange?: (policy: TaskPermissionPolicy) => Promise<void>;
  permissionPolicyDisabled?: boolean;
  permissionPolicyDisabledReason?: string;
  workspaceRoot: string;
  showWorkspaceContext?: boolean;
  worktreeName?: string;
  gitRef?: string;
  agentNavigation?: ReactNode;
}) {
  const statusPresentation = STATUS_PRESENTATION[status];
  const visibleStatusLabel = statusLabel ?? statusPresentation.label;
  const StatusIcon = statusPresentation.Icon;
  const projectLabel = workspaceRoot.trim() ? workspaceLabel(workspaceRoot) : undefined;
  const dragRegionProps = desktopWindow ? desktopDragRegionProps(desktopWindow) : undefined;
  return (
    <header
      className="task-header"
      data-desktop-window={desktopWindow?.platform}
      {...dragRegionProps}
    >
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
          {agentNavigation ?? <span className="task-header-agent">
            <AgentIcon agentId={agentId} agentName={agentName} size={11} />
            <span>{agentName}</span>
          </span>}
        </span>
      </span>
      {permissionPolicy && onPermissionPolicyChange ? (
        <TaskPermissionPolicyControl
          disabled={permissionPolicyDisabled}
          disabledReason={permissionPolicyDisabledReason}
          onChange={onPermissionPolicyChange}
          policy={permissionPolicy}
        />
      ) : showWorkspaceContext && (worktreeName || projectLabel) ? <span className="task-header-workspace" title={[workspaceRoot, gitRef].filter(Boolean).join("\n")}>
        {worktreeName ? <GitBranch size={12} /> : <FolderRoot size={12} />}
        {worktreeName ?? "Project root"}
      </span> : null}
    </header>
  );
}
