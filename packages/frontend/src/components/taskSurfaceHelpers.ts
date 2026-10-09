import type {
  AgentListedSession,
  BackgroundCommand,
  ChatMessage,
  HistorySyncState,
  TaskSnapshot,
  TaskStatus,
} from "@openaide/app-shell-contracts";
import { activityStepCompletedLabel, activityStepProgressLabel, activityStepWithTitle } from "../state/activityLabels";

export function newTaskStatusLabel({
  openingNativeSession,
  submitting,
}: {
  openingNativeSession?: boolean;
  submitting: boolean;
}) {
  if (openingNativeSession) return "Opening task";
  if (submitting) return "Starting task";
  return undefined;
}

/** A turn the user can steer or Stop: the Agent is working or only its Background Work remains. */
export function taskTurnOpen(status: TaskStatus | undefined) {
  return status === "active" || status === "background";
}

/** Summary of the Background Work row; zero means a finished command's follow-up is awaited. */
export function backgroundWorkLabel(commandCount: number) {
  if (!commandCount) return "Background command finished";
  return commandCount === 1 ? "1 background command running" : `${commandCount} background commands running`;
}

export function taskWorkingStatusLabel(
  items: ChatMessage[],
  status: TaskStatus,
  inputPending: boolean,
  historySync: HistorySyncState = { state: "idle", generation: 0 },
) {
  if (historySync.state === "syncing") return "Reloading session";
  if (historySync.state === "updated") return "History updated";
  // Pending Shell input remains in the frozen composer until App Server acceptance.
  // Chat activity only describes authoritative task state.
  if (inputPending) return undefined;
  if (status === "stopping") return "Stopping";
  if (items.some((item) => (
    (item.message.kind === "permission" || item.message.kind === "elicitation")
    && item.message.state === "pending"
  ))) return undefined;
  if (status === "waiting") {
    if (items.some((item) => item.message_id === "app-server-preparation")) {
      return "Preparing task";
    }
    if (items.some((item) => (
      item.message_id === "app-server-send-capability"
      || item.message_id.startsWith("app-server-preparation-")
    ))) {
      // Chat already renders these authoritative interruptions and their recovery actions.
      return undefined;
    }
    return "Permission needed";
  }
  // A held turn is described by the Background Work row, not by a status line.
  if (status !== "active") return undefined;
  // A new user message starts a new turn; completed work before it must not leak into the live footer.
  const reversedUserIndex = [...items].reverse().findIndex((item) => item.message.kind === "user");
  const currentTurnItems = reversedUserIndex === -1 ? items : items.slice(items.length - reversedUserIndex);
  const latestWork = [...currentTurnItems].reverse().find((item) => {
    return item.message.kind === "activity"
      || item.message.kind === "agent_message"
      || item.message.kind === "compaction";
  });
  if (latestWork?.message.kind === "compaction") {
    // An in-progress Compaction row is itself the live indicator; a second footer would compete with
    // it. Once it ends, the Tool that preceded it is no longer what the Agent is doing.
    return latestWork.message.status === "in_progress" ? undefined : "Working";
  }
  if (latestWork?.message.kind === "agent_message") {
    return latestWork.message.role === "thought" ? "Thinking" : "Writing response";
  }
  if (latestWork?.message.kind === "activity") {
    // The footer tracks the newest concrete action while the folded group keeps its broader title.
    const latestStep = [...latestWork.message.steps]
      .reverse()
      .find((candidate) => candidate.kind === "tool" || candidate.kind === "command" || candidate.kind === "thought");
    if (!latestStep) return "Working";
    const step = activityStepWithTitle(latestStep,
      latestWork.message.steps.length === 1 ? latestWork.message.title : undefined);
    if (step.kind === "thought" && step.streaming) return activityStepProgressLabel(step, latestWork.message.title);
    if (step.kind !== "thought" && step.status === "running") {
      return activityStepProgressLabel(step, latestWork.message.title);
    }
    return activityStepCompletedLabel(step);
  }
  return "Starting";
}

export function relativeTime(value: string) {
  const timestamp = timestampMillis(value);
  if (Number.isNaN(timestamp)) return "";
  const seconds = Math.max(0, Math.floor((Date.now() - timestamp) / 1000));
  if (seconds < 60) return "now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  const days = Math.floor(hours / 24);
  return `${days}d`;
}

export function nativeSessionTitle(session: AgentListedSession) {
  const title = session.title?.trim();
  return title || "Untitled task";
}

export function nativeSessionMeta(session: AgentListedSession, agentName: string) {
  const parts = [];
  parts.push(agentName);
  const lastActivity = session.last_activity ?? session.updated_at;
  if (lastActivity) {
    const updated = relativeTime(lastActivity);
    if (updated) parts.push(updated);
  }
  return parts.join(" · ");
}

export function workspaceLabel(root: string) {
  const normalized = root.replace(/\\/g, "/").replace(/\/+$/, "");
  const label = normalized.split("/").filter(Boolean).pop();
  return label || "Workspace";
}

/** Parses both persisted Unix-millisecond strings and ISO timestamps from App Server data. */
export function timestampMillis(value: string) {
  const trimmed = value.trim();
  if (/^\d+$/.test(trimmed)) return Number(trimmed);
  return Date.parse(trimmed);
}

// One identity for "no commands", so memoized Chat rows do not churn on every render.
const NO_BACKGROUND_COMMANDS: BackgroundCommand[] = [];

/** Background Work of the active turn as the Task Page presents it. */
export function taskBackgroundWork(snapshot: TaskSnapshot, hidden: boolean) {
  const commands = snapshot.background_commands ?? NO_BACKGROUND_COMMANDS;
  const held = snapshot.task.status === "background";
  return {
    commands,
    // Listed while any command is alive; a held turn shows the row even when it only awaits a follow-up.
    shown: !hidden && (commands.length > 0 || held),
    cancelLabel: held && commands.length > 0
      ? `Stop ${commands.length} background ${commands.length === 1 ? "command" : "commands"}`
      : undefined,
  };
}
