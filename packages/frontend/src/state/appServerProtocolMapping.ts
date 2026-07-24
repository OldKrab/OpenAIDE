import type {
  AgentSummary,
  ProjectSummary,
  TaskNavigationSnapshot as ProtocolTaskNavigationSnapshot,
  TaskSnapshot as ProtocolTaskSnapshot,
  TaskStatus as ProtocolTaskStatus,
  TaskSummary as ProtocolTaskSummary,
} from "@openaide/app-server-client";
import type {
  AgentListedSession,
  AgentCommandsCatalog,
  ChatMessage,
  ConfigOptionsCatalog,
  TaskSnapshot,
  TaskSummary,
} from "@openaide/app-shell-contracts";
import {
  mapProtocolChatItem,
  pendingRequestItems,
  recoveryItems,
  systemInterruptionItem,
} from "./appServerProtocolChatMapping";

const DEFAULT_LOCAL_ISOLATION = "local" as const;

export type ProtocolMappingContext = {
  agents?: AgentSummary[];
  projects?: ProjectSummary[];
};

export type ProtocolMappingWarning =
  | { kind: "pendingRequestsNeedNativeSurface"; requestIds: string[] }
  | { kind: "recoveryMappedToInterruption"; actions: string[] }
  | { kind: "preparationNeedsNativeSurface"; state: ProtocolTaskSnapshot["preparation"]["kind"] }
  | { kind: "sendCapabilityNeedsNativeSurface"; state: ProtocolTaskSnapshot["sendCapability"]["state"] }
  | { kind: "agentCommandsNeedNativeSurface"; state: ProtocolTaskSnapshot["agentCommands"]["state"] }
  | { kind: "projectDisplayNotMapped"; projectId: string }
  | { kind: "agentLabelMissing"; agentId: string };

export type ProtocolTaskNavigationMapping = {
  tasks: TaskSummary[];
  sessions: AgentListedSession[];
  hasMoreProjectIds: string[];
  refreshing: boolean;
  refreshError?: string;
  activeTaskId?: string;
  warnings: ProtocolMappingWarning[];
  requiresNativeSurface: boolean;
};

export type ProtocolTaskSnapshotMapping = {
  snapshot: TaskSnapshot;
  warnings: ProtocolMappingWarning[];
  requiresNativeSurface: boolean;
};

export class AppServerCompatibilityError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "AppServerCompatibilityError";
  }
}

type ProtocolChatItem = ProtocolTaskSnapshot["chat"]["items"][number];
type ChatItemProjectionCache = WeakMap<ProtocolChatItem, ChatMessage>;

/**
 * Maps a subscription's structurally shared protocol replica without changing
 * the identity of historical Chat rows on every focused Task event.
 */
export function createProtocolTaskSnapshotMapper() {
  const chatItems: ChatItemProjectionCache = new WeakMap();
  return (
    snapshot: ProtocolTaskSnapshot,
    context: ProtocolMappingContext = {},
  ) => mapProtocolTaskSnapshotWithCache(snapshot, context, chatItems);
}

export function mapProtocolTaskNavigation(
  snapshot: ProtocolTaskNavigationSnapshot,
  context: ProtocolMappingContext = {},
): ProtocolTaskNavigationMapping {
  if (!Array.isArray(snapshot?.groups)) {
    throw new AppServerCompatibilityError(
      "OpenAIDE received an incompatible App Server response. Reload the VS Code window.",
    );
  }
  const entries = snapshot.groups.flatMap((group) => group.entries);
  const taskEntries = entries
    .filter((entry) => entry.kind === "task")
    .map((entry) => entry.task);
  const mapped = taskEntries
    .map((task) => mapProtocolTaskSummaryWithWarnings(task, 0, context));
  const sessions = entries.flatMap((entry): AgentListedSession[] => {
    if (entry.kind !== "nativeSession") return [];
    return [{
      agent_id: entry.session.reference.agentId,
      agent_name: context.agents?.find((agent) => agent.agentId === entry.session.reference.agentId)?.label
        ?? entry.session.reference.agentId,
      session_id: entry.session.reference.sessionId,
      project_id: entry.session.projectId,
      cwd: entry.session.workspaceRoot,
      title: entry.session.title ?? undefined,
      last_activity: entry.session.lastActivity ?? undefined,
    }];
  });
  return {
    tasks: mapped.map((item) => item.task),
    sessions,
    hasMoreProjectIds: snapshot.groups
      .filter((group) => group.hasMore)
      .map((group) => group.projectId),
    refreshing: snapshot.refresh.state === "refreshing",
    refreshError: snapshot.refresh.state === "failed" ? snapshot.refresh.message : undefined,
    activeTaskId: undefined,
    warnings: mapped.flatMap((item) => item.warnings),
    requiresNativeSurface: false,
  };
}

export function mapProtocolTaskSnapshot(
  snapshot: ProtocolTaskSnapshot,
  context: ProtocolMappingContext = {},
): ProtocolTaskSnapshotMapping {
  return mapProtocolTaskSnapshotWithCache(snapshot, context);
}

function mapProtocolTaskSnapshotWithCache(
  snapshot: ProtocolTaskSnapshot,
  context: ProtocolMappingContext,
  chatItemCache?: ChatItemProjectionCache,
): ProtocolTaskSnapshotMapping {
  const mappedTask = mapProtocolTaskSummaryWithWarnings(
    snapshot.task,
    snapshot.revision,
    context,
    snapshot.lifecycle === "prepared" ? "New task" : "Untitled task",
  );
  const task = mappedTask.task;
  const items = snapshot.chat.items.map((item) => {
    const cached = chatItemCache?.get(item);
    if (cached) return cached;
    const mapped = mapProtocolChatItem(item, task.updated_at);
    chatItemCache?.set(item, mapped);
    return mapped;
  });
  const extraItems = [
    ...sendCapabilityItems(snapshot, task.updated_at),
    ...recoveryItems(snapshot.recovery, task.updated_at),
  ];
  const allItems = [...items, ...extraItems];
  const warnings = [
    ...mappedTask.warnings,
    ...snapshotWarnings(snapshot),
  ];
  const sendBlockers = snapshot.sendCapability.blockers ?? [];

  return {
    snapshot: {
      lifecycle: snapshot.lifecycle,
      task: taskWithCapabilityStatus(task, snapshot),
      chat: {
        task_id: task.task_id,
        items: allItems,
        has_before: snapshot.chat.hasMoreBefore === true,
        has_messages: snapshot.chat.hasMessages,
        total_count: snapshot.chat.hasMessages ? Math.max(allItems.length, 1) : 0,
        version: snapshot.revision,
        start_cursor: snapshot.chat.startCursor ?? allItems[0]?.cursor,
        end_cursor: snapshot.chat.endCursor ?? allItems.at(-1)?.cursor,
      },
      active_turn_started_at: snapshot.activeTurnStartedAt ?? undefined,
      active_requests: pendingRequestItems(snapshot.pendingRequests ?? [], task.updated_at),
      settings_summary: {
        agent_id: task.agent_id,
        isolation: DEFAULT_LOCAL_ISOLATION,
      },
      agent_config: mapProtocolConfigOptions(snapshot.agentConfig, task.agent_id),
      agent_commands: mapProtocolAgentCommands(snapshot.agentCommands, task.agent_id),
      preparation: mapTaskPreparation(snapshot.preparation),
      send_capability: {
        state: snapshot.sendCapability.state,
        ...(sendBlockers.length > 0
          ? { blockers: sendBlockers.map((blocker) => ({ ...blocker })) }
          : {}),
      },
      input_capabilities: {
        image: snapshot.inputCapabilities?.image ?? false,
      },
      revision: snapshot.revision,
      history_sync: mapHistorySync(snapshot.historySync ?? { state: "idle", generation: 0 }),
    },
    warnings,
    requiresNativeSurface: warnings.some(requiresNativeSurface),
  };
}

function mapTaskPreparation(
  preparation: ProtocolTaskSnapshot["preparation"],
): NonNullable<TaskSnapshot["preparation"]> {
  switch (preparation.kind) {
    case "blocked":
      return { kind: "blocked", blocker: { kind: preparation.blocker.kind } };
    case "failed":
      return preparation.error.code === "notFound"
        ? { kind: "failed", recovery: "replaceTask" }
        : { kind: "failed" };
    case "preparing":
      return { kind: "preparing" };
    case "ready":
      return { kind: "ready" };
  }
}

function mapHistorySync(sync: ProtocolTaskSnapshot["historySync"]): NonNullable<TaskSnapshot["history_sync"]> {
  switch (sync.state) {
    case "syncing": return { state: "syncing", generation: sync.generation };
    case "updated": return { state: "updated", generation: sync.generation };
    case "idle": return { state: "idle", generation: sync.generation };
  }
}

export function mapProtocolTaskSummary(
  summary: ProtocolTaskSummary,
  revision = 0,
  context: ProtocolMappingContext = {},
): TaskSummary {
  return mapProtocolTaskSummaryWithWarnings(summary, revision, context).task;
}

export function mapProtocolConfigOptions(
  snapshot: ProtocolTaskSnapshot["agentConfig"],
  agentId: string,
): ConfigOptionsCatalog {
  const options = (snapshot.options ?? []).map((option) => ({
    id: option.configId,
    label: option.label,
    description: option.description ?? undefined,
    category: configCategoryFromProtocol(option.category),
    kind: option.kind,
    current_value: option.currentValue,
    values: option.values.map((value) => ({
      id: value.value,
      label: value.label,
      description: value.description ?? undefined,
    })),
  }));
  return {
    agent_id: agentId,
    status: snapshot.state === "ready" ? (options.length ? "ready" : "empty") : snapshot.state,
    options,
    pending_change: snapshot.pendingChange ? {
      mutation_id: snapshot.pendingChange.clientMutationId,
      option_id: snapshot.pendingChange.configId,
      requested_value: snapshot.pendingChange.requestedValue,
    } : undefined,
    error: snapshot.error?.message,
  };
}

export function mapProtocolAgentCommands(
  snapshot: ProtocolTaskSnapshot["agentCommands"],
  agentId: string,
): AgentCommandsCatalog | undefined {
  if (snapshot.state !== "ready") return undefined;
  const commands = (snapshot.commands ?? []).map((command) => ({
    name: command.name,
    description: command.description,
    input_hint: command.input?.hint ?? undefined,
  }));
  return {
    agent_id: agentId,
    status: commands.length ? "ready" : "empty",
    commands,
  };
}

function mapProtocolTaskSummaryWithWarnings(
  summary: ProtocolTaskSummary,
  revision: number,
  context: ProtocolMappingContext,
  fallbackTitle = "Untitled task",
): { task: TaskSummary; warnings: ProtocolMappingWarning[] } {
  const agent = context.agents?.find((candidate) => candidate.agentId === summary.agentId);
  const project = context.projects?.find((candidate) => candidate.projectId === summary.projectId);
  const warnings: ProtocolMappingWarning[] = [];
  const agentLabel = agent?.label ?? knownBuiltInAgentLabel(summary.agentId) ?? summary.agentId;
  if (!agent) warnings.push({ kind: "agentLabelMissing", agentId: summary.agentId });
  if (!project) warnings.push({ kind: "projectDisplayNotMapped", projectId: summary.projectId });
  const lastActivity = summary.lastActivity ?? summary.updatedAt;

  return {
    task: {
      task_id: summary.taskId,
      project_id: summary.projectId,
      project_label: project?.label,
      title: summary.title?.value ?? fallbackTitle,
      title_source: summary.title?.source,
      status: taskSummaryStatusFromProtocol(summary.status),
      task_version: revision,
      message_history_version: revision,
      has_messages: summary.hasMessages,
      unread: summary.unread,
      attention: summary.attention ? {
        event_id: summary.attention.eventId,
        reason: summary.attention.reason,
        occurred_at: summary.attention.occurredAt,
      } : undefined,
      created_at: summary.updatedAt,
      updated_at: summary.updatedAt,
      last_activity: lastActivity,
      agent_id: summary.agentId,
      agent_name: agentLabel,
      isolation: DEFAULT_LOCAL_ISOLATION,
      workspace_root: "",
      worktree_id: summary.worktreeId ?? undefined,
      workspace_available: summary.workspaceAvailable,
    },
    warnings,
  };
}

function knownBuiltInAgentLabel(agentId: string) {
  switch (agentId) {
    case "codex":
      return "Codex";
    case "opencode":
      return "OpenCode";
    default:
      return undefined;
  }
}

function taskWithCapabilityStatus(task: TaskSummary, snapshot: ProtocolTaskSnapshot): TaskSummary {
  if (snapshot.preparation.kind === "preparing") {
    return { ...task, status: "active" };
  }
  if (snapshot.preparation.kind === "blocked" || snapshot.preparation.kind === "failed") {
    return { ...task, status: snapshot.preparation.kind === "failed" ? "failed" : "waiting" };
  }
  if (sendCapabilityBlockedByTaskWork(snapshot) || sendCapabilityBlockedByTaskPreparation(snapshot)) return task;
  if (snapshot.sendCapability.state !== "ready") return { ...task, status: "waiting" };
  return task;
}

function taskSummaryStatusFromProtocol(status: ProtocolTaskStatus): TaskSummary["status"] {
  switch (status) {
    case "running":
    case "starting":
    case "preparing":
      return "active";
    case "stopping":
      return "stopping";
    case "waiting":
      return "waiting";
    case "failed":
    case "interrupted":
      return "failed";
    case "completed":
      return "completed";
    case "idle":
      return "inactive";
  }
}

function sendCapabilityItems(snapshot: ProtocolTaskSnapshot, createdAt: string) {
  if (
    snapshot.sendCapability.state === "ready"
    || sendCapabilityBlockedByTaskWork(snapshot)
    || sendCapabilityBlockedByTaskPreparation(snapshot)
  ) return [];
  const message = snapshot.sendCapability.blockers?.map((blocker) => blocker.message).join(" ") || "Sending is not available.";
  return [systemInterruptionItem("app-server-send-capability", message, createdAt, snapshot.sendCapability.state !== "failed")];
}

function configCategoryFromProtocol(category: string | null | undefined): ConfigOptionsCatalog["options"][number]["category"] {
  switch (category) {
    case "mode":
    case "model":
    case "thought_level":
      return category;
    default:
      return "other";
  }
}

function snapshotWarnings(snapshot: ProtocolTaskSnapshot): ProtocolMappingWarning[] {
  const warnings: ProtocolMappingWarning[] = [];
  const taskRequests = (snapshot.pendingRequests ?? []).filter((request) => (
    request.scope.kind === "task"
    && !(
      (request.kind === "permission" && request.permission)
      || (request.kind === "question" && request.question)
    )
  ));
  if (taskRequests.length) {
    warnings.push({
      kind: "pendingRequestsNeedNativeSurface",
      requestIds: taskRequests.map((request) => request.requestId),
    });
  }
  if (snapshot.recovery) {
    warnings.push({ kind: "recoveryMappedToInterruption", actions: snapshot.recovery.actions });
  }
  if (snapshot.preparation.kind !== "ready") {
    warnings.push({ kind: "preparationNeedsNativeSurface", state: snapshot.preparation.kind });
  }
  if (snapshot.sendCapability.state !== "ready" && !sendCapabilityBlockedByTaskWork(snapshot)) {
    warnings.push({ kind: "sendCapabilityNeedsNativeSurface", state: snapshot.sendCapability.state });
  }
  if (snapshot.agentCommands.state !== "ready") {
    warnings.push({ kind: "agentCommandsNeedNativeSurface", state: snapshot.agentCommands.state });
  }
  return warnings;
}

function sendCapabilityBlockedByTaskWork(snapshot: ProtocolTaskSnapshot) {
  return snapshot.sendCapability.state === "blocked" &&
    snapshot.sendCapability.blockers?.some((blocker) => blocker.kind === "taskRunning") === true;
}

function sendCapabilityBlockedByTaskPreparation(snapshot: ProtocolTaskSnapshot) {
  const blockers = snapshot.sendCapability.blockers ?? [];
  return (snapshot.sendCapability.state === "loading" || snapshot.sendCapability.state === "blocked")
    && blockers.length > 0
    && blockers.every((blocker) => blocker.kind === "taskPreparing");
}

function requiresNativeSurface(warning: ProtocolMappingWarning) {
  switch (warning.kind) {
    case "pendingRequestsNeedNativeSurface":
    case "preparationNeedsNativeSurface":
    case "sendCapabilityNeedsNativeSurface":
    case "agentCommandsNeedNativeSurface":
    case "recoveryMappedToInterruption":
      return true;
    case "projectDisplayNotMapped":
    case "agentLabelMissing":
      return false;
  }
}
