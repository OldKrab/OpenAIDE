import type { AgentIconId, Attachment, ConfigOptionsCatalog, IsolationKind } from "@openaide/app-shell-contracts";
import { projectIdForWorkspaceRoot } from "@openaide/app-shell-contracts";
import type { AttachmentHandleId, PreSendAttachment } from "@openaide/app-server-client";
import { agentCatalogEntry, builtInAgents, defaultAgent } from "@openaide/app-shell-contracts";

export type AgentOption = {
  id: string;
  label: string;
  description: string;
  icon: AgentIconId;
  enabled?: boolean;
};

export type IsolationOption = {
  id: IsolationKind;
  label: string;
  description: string;
};

export type WorkspaceRoot = {
  path: string;
  label: string;
  projectId?: string;
};

export type ProjectOption = {
  projectId: string;
  label: string;
};

export type ComposerAttachment = Attachment & {
  local_id: string;
  app_server_handle_id?: AttachmentHandleId;
  preview_url?: string;
  validation_error?: string;
};

export type ComposerSelection = {
  agentId: string;
  agentLabel: string;
  isolation: IsolationKind;
  configOptions: Record<string, string>;
  projectId?: string;
  workspaceRoot: string;
  workspaceLabel: string;
};

export const agentOptions: AgentOption[] = [
  ...builtInAgents.map((agent) => ({
    id: agent.id,
    label: agent.label,
    description: agent.description,
    icon: agent.icon,
  })),
];

export const isolationOptions: IsolationOption[] = [
  { id: "local", label: "Local", description: "Use the current workspace." },
  { id: "git_worktree", label: "Worktree", description: "Use a separate git worktree when supported." },
  { id: "docker", label: "Docker", description: "Use container isolation when supported." },
];

export function defaultSelection(workspace?: WorkspaceRoot): ComposerSelection {
  return {
    agentId: defaultAgent.id,
    agentLabel: defaultAgent.label,
    isolation: "local",
    configOptions: {},
    projectId: workspace?.projectId,
    workspaceRoot: workspace?.path ?? "",
    workspaceLabel: workspace?.label ?? "Workspace",
  };
}

export function selectionWithAgent(selection: ComposerSelection, agentId: string, label?: string): ComposerSelection {
  const option = agentCatalogEntry(agentId) ?? defaultAgent;
  if (label) return { ...selection, agentId, agentLabel: label };
  return { ...selection, agentId: option.id, agentLabel: option.label };
}

export function selectionWithIsolation(selection: ComposerSelection, isolation: IsolationKind): ComposerSelection {
  return { ...selection, isolation };
}

export function selectionWithConfigOptions(
  selection: ComposerSelection,
  catalog: ConfigOptionsCatalog,
): ComposerSelection {
  return {
    ...selection,
    configOptions: Object.fromEntries(
      catalog.options.map((option) => [option.id, option.current_value]),
    ),
  };
}

export function selectionWithWorkspace(selection: ComposerSelection, workspace: WorkspaceRoot): ComposerSelection {
  return {
    ...selection,
    projectId: workspace.projectId ?? projectIdForWorkspaceRoot(workspace.path),
    workspaceRoot: workspace.path,
    workspaceLabel: workspace.label,
  };
}

export function selectionWithProject(selection: ComposerSelection, project: ProjectOption): ComposerSelection {
  return {
    ...selection,
    projectId: project.projectId,
    workspaceLabel: project.label,
  };
}

export function localAttachment(attachment: Attachment): ComposerAttachment {
  return {
    ...attachment,
    local_id: `attachment_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
  };
}

export function appServerAttachment(
  attachment: PreSendAttachment,
  options: { previewUrl?: string } = {},
): ComposerAttachment {
  return {
    kind: "file",
    label: attachment.label,
    local_id: `attachment_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
    app_server_handle_id: attachment.handleId,
    preview_url: options.previewUrl,
  };
}

export function protocolAttachments(attachments: ComposerAttachment[]): Attachment[] {
  return attachments.map(({
    local_id: localId,
    app_server_handle_id: _handleId,
    preview_url: previewUrl,
    validation_error: _validationError,
    ...attachment
  }) => ({
    ...attachment,
    id: localId,
    ...(previewUrl ? { payload: { previewUrl } } : {}),
  }));
}

export function appServerAttachmentHandles(attachments: ComposerAttachment[]): AttachmentHandleId[] | undefined {
  const handles = attachments.map((attachment) => attachment.app_server_handle_id);
  return handles.every((handle): handle is AttachmentHandleId => Boolean(handle)) ? handles : undefined;
}

/** Preserves the visible draft while removing resolver ids the App Server no longer recognizes. */
export function invalidateAppServerAttachments(
  attachments: ComposerAttachment[],
  message: string,
): ComposerAttachment[] {
  return attachments.map(({ app_server_handle_id: _handleId, ...attachment }) => ({
    ...attachment,
    validation_error: message,
  }));
}
