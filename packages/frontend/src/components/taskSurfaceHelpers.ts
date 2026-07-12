import type { AgentListedSession, ChatMessage, HistorySyncState, TaskStatus } from "@openaide/app-shell-contracts";
import { activityStepCompletedLabel, activityStepProgressLabel } from "../state/activityLabels";

export function newTaskStatusLabel({
  agentLabel,
  configOptionsError,
  configOptionsLoading,
  configOptionsReady,
  needsWorkspace,
  openingNativeSession,
  submitting,
}: {
  agentLabel: string;
  configOptionsError?: string;
  configOptionsLoading?: boolean;
  configOptionsReady: boolean;
  needsWorkspace: boolean;
  openingNativeSession?: boolean;
  submitting: boolean;
}) {
  if (openingNativeSession) return "Opening task";
  if (submitting) return "Starting task";
  if (needsWorkspace || configOptionsError) return undefined;
  if (configOptionsLoading || !configOptionsReady) return `Preparing ${agentLabel} options`;
  return undefined;
}

export function taskWorkingStatusLabel(
  items: ChatMessage[],
  status: TaskStatus,
  inputPending: boolean,
  historySync: HistorySyncState = { state: "idle", generation: 0 },
) {
  if (historySync.state === "checking") return "Checking for newer history";
  if (historySync.state === "syncing") return "Syncing conversation history";
  if (historySync.state === "updated") return "History updated";
  if (historySync.state === "failed") {
    return historySync.before_send
      ? "Couldn’t sync conversation history"
      : "Couldn’t refresh history";
  }
  // Pending Shell input remains in the frozen composer until App Server acceptance.
  // Chat activity only describes authoritative task state.
  if (inputPending) return undefined;
  if (items.some((item) => (
    (item.message.kind === "permission" || item.message.kind === "elicitation")
    && item.message.state === "pending"
  ))) return undefined;
  if (status === "blocked") {
    if (items.some((item) => item.message_id === "app-server-preparation")) {
      return "Preparing task";
    }
    if (items.some((item) => item.message_id === "app-server-send-capability")) {
      return "Sending is not available";
    }
    return "Permission needed";
  }
  if (status !== "active") return undefined;
  // A new user message starts a new turn; completed work before it must not leak into the live footer.
  const reversedUserIndex = [...items].reverse().findIndex((item) => item.message.kind === "user");
  const currentTurnItems = reversedUserIndex === -1 ? items : items.slice(items.length - reversedUserIndex);
  const latestWork = [...currentTurnItems].reverse().find((item) => {
    return item.message.kind === "activity" || item.message.kind === "agent_text" || item.message.kind === "thought";
  });
  if (latestWork?.message.kind === "thought") {
    return latestWork.message.streaming
      ? activityStepProgressLabel(latestWork.message)
      : activityStepCompletedLabel(latestWork.message);
  }
  if (latestWork?.message.kind === "agent_text") {
    return latestWork.message.streaming ? "Writing response" : "Generated response";
  }
  if (latestWork?.message.kind === "activity") {
    // The footer tracks the newest concrete action while the folded group keeps its broader title.
    const step = [...latestWork.message.steps]
      .reverse()
      .find((candidate) => candidate.kind === "tool" || candidate.kind === "command" || candidate.kind === "thought");
    if (!step) return "Working";
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

function timestampMillis(value: string) {
  const trimmed = value.trim();
  if (/^\d+$/.test(trimmed)) return Number(trimmed);
  return Date.parse(trimmed);
}
