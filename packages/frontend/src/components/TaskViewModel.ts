import type { TaskStatus } from "@openaide/app-shell-contracts";
import type { TaskChatScrollState } from "../state/store";

export function taskComposerAvailability({
  archived = false,
  backendReady,
  connectionStatus,
  inputPending,
  inputUncertain = false,
  preparationBlocked,
  sendCapabilityState = "ready",
  taskStatus,
}: {
  archived?: boolean;
  backendReady: boolean;
  connectionStatus?: "connecting" | "ready" | "reconnecting" | "unavailable";
  inputPending: boolean;
  inputUncertain?: boolean;
  preparationBlocked: boolean;
  sendCapabilityState?: "loading" | "ready" | "blocked" | "failed";
  taskStatus: TaskStatus;
}) {
  const turnBusy = taskStatus === "active" || taskStatus === "blocked";
  const keepingDraftAvailable = !backendReady
    && (connectionStatus === "reconnecting" || connectionStatus === "unavailable");
  const editingDisabled = archived || (!backendReady && !keepingDraftAvailable) || inputPending || inputUncertain || preparationBlocked;
  const sendDisabled = archived
    || !backendReady
    || inputPending
    || preparationBlocked
    || sendCapabilityState !== "ready";
  if (archived) return { editingDisabled, sendDisabled, placeholder: "Restore task to send follow-up." };
  if (preparationBlocked) return { editingDisabled, sendDisabled, placeholder: "Preparing task." };
  if (!backendReady) {
    return {
      editingDisabled,
      sendDisabled,
      placeholder: keepingDraftAvailable ? "Reconnecting. Draft is saved here." : "Connecting to App Server.",
    };
  }
  if (inputPending) return { editingDisabled, sendDisabled, placeholder: "Sending." };
  if (inputUncertain) {
    return {
      editingDisabled,
      sendDisabled: !backendReady,
      placeholder: "Retry this exact message.",
    };
  }
  if (taskStatus === "blocked") {
    return { editingDisabled, sendDisabled, placeholder: "Draft follow-up while input is pending." };
  }
  if (turnBusy) return { editingDisabled, sendDisabled, placeholder: "Send a follow-up" };
  return { editingDisabled, sendDisabled, placeholder: "Send follow-up" };
}

/** Returns the only programmatic position owned by Chat; readers keep their current viewport. */
export function scrollTopForFollowingViewport({
  clientHeight,
  ownership,
  scrollHeight,
}: {
  clientHeight: number;
  ownership: TaskChatScrollState["ownership"];
  scrollHeight: number;
}) {
  return ownership === "following" ? Math.max(0, scrollHeight - clientHeight) : undefined;
}

export function scrollTopAfterPrependedContent({
  nextScrollHeight,
  previousScrollHeight,
  previousScrollTop,
}: {
  nextScrollHeight: number;
  previousScrollHeight: number;
  previousScrollTop: number;
}) {
  return previousScrollTop + Math.max(0, nextScrollHeight - previousScrollHeight);
}
