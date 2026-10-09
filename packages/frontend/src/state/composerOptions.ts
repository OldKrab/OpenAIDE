import type { AgentIconId, Attachment, IsolationKind } from "@openaide/app-shell-contracts";
import { projectIdForWorkspaceRoot } from "@openaide/app-shell-contracts";
import type { AttachmentHandleId, ComposerImage, PreSendAttachment } from "@openaide/app-server-client";
import { agentCatalogEntry, builtInAgents, defaultAgent } from "@openaide/app-shell-contracts";

export type AgentOption = {
  id: string;
  label: string;
  description: string;
  icon: AgentIconId;
  enabled?: boolean;
  status?: import("@openaide/app-server-client").AgentStatus;
  setupReason?: import("@openaide/app-server-client").AgentSetupReason;
  capabilities?: import("@openaide/app-server-client").AgentCapabilities;
  /** Account-wide usage limits the Agent reported; shared by every Task of this Agent. */
  accountLimits?: import("@openaide/app-server-client").AgentAccountLimits;
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
  workspaceRoot?: string;
  available?: boolean;
  worktreeRepositoryId?: string;
  projectWorktreeId?: string;
  worktreeError?: string;
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
  projectId?: string;
  workspaceRoot: string;
  workspaceLabel: string;
  worktreeId?: string;
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
    agentId: "",
    agentLabel: "",
    isolation: "local",
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
    workspaceRoot: project.workspaceRoot ?? selection.workspaceRoot,
    worktreeId: undefined,
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

/** Moves one draft row to `targetIndex`; row order is the order Send delivers within each content kind. */
export function moveAttachment(
  attachments: ComposerAttachment[],
  attachmentId: string,
  targetIndex: number,
): ComposerAttachment[] {
  const fromIndex = attachments.findIndex((attachment) => attachment.local_id === attachmentId);
  if (fromIndex < 0) return attachments;
  const clampedIndex = Math.max(0, Math.min(targetIndex, attachments.length - 1));
  if (clampedIndex === fromIndex) return attachments;
  const reordered = [...attachments];
  const [attachment] = reordered.splice(fromIndex, 1);
  reordered.splice(clampedIndex, 0, attachment);
  return reordered;
}

/** Merges both New Task draft sources into the one ordered list the Composer renders and sends. */
export function newTaskDraftAttachments(
  newTask: { context: ComposerAttachment[]; contextOrder?: string[] },
  preparedTaskAttachments: ComposerAttachment[] = [],
): ComposerAttachment[] {
  const merged = [
    ...newTask.context,
    ...preparedTaskAttachments.filter((prepared) =>
      !newTask.context.some((local) => local.local_id === prepared.local_id)),
  ];
  if (!newTask.contextOrder) return merged;
  // Rows added after the last reorder keep arrival order behind the ranked rows.
  const rank = new Map(newTask.contextOrder.map((id, index) => [id, index]));
  return merged
    .map((attachment, index) => ({ attachment, index }))
    .sort((left, right) => (
      (rank.get(left.attachment.local_id) ?? Number.MAX_SAFE_INTEGER)
        - (rank.get(right.attachment.local_id) ?? Number.MAX_SAFE_INTEGER)
      || left.index - right.index
    ))
    .map(({ attachment }) => attachment);
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

type ComposerImagePayload = {
  data: string;
  mimeType: string;
};

/** Creates one client-owned Image; no Task or App Server resource exists before Send. */
export function localImageAttachment(file: File, data: string): ComposerAttachment {
  const mimeType = file.type || "image/png";
  return {
    kind: "image",
    label: file.name || "Image",
    local_id: `image_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
    preview_url: `data:${mimeType};base64,${data}`,
    payload: { data, mimeType } satisfies ComposerImagePayload,
  };
}

/** Converts only valid client-owned Images into the inline task/send representation. */
export function appServerComposerImages(attachments: ComposerAttachment[]): ComposerImage[] | undefined {
  const images = attachments.map((attachment) => {
    if (attachment.kind !== "image" || !isComposerImagePayload(attachment.payload)) return undefined;
    return {
      label: attachment.label,
      mimeType: attachment.payload.mimeType,
      data: attachment.payload.data,
    } satisfies ComposerImage;
  });
  return images.every((image): image is ComposerImage => image !== undefined) ? images : undefined;
}

function isComposerImagePayload(payload: unknown): payload is ComposerImagePayload {
  if (!payload || typeof payload !== "object") return false;
  const candidate = payload as Partial<ComposerImagePayload>;
  return typeof candidate.data === "string" && typeof candidate.mimeType === "string";
}

/** Preserves the visible draft while removing resolver ids the App Server no longer recognizes. */
export function invalidateAppServerAttachments(
  attachments: ComposerAttachment[],
  message: string,
): ComposerAttachment[] {
  return attachments.map((attachment) => {
    if (!attachment.app_server_handle_id) return attachment;
    const { app_server_handle_id: _handleId, ...visibleAttachment } = attachment;
    return {
      ...visibleAttachment,
      validation_error: message,
    };
  });
}
