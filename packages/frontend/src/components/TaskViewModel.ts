import type { TaskStatus } from "@openaide/app-shell-contracts";

export function taskComposerAvailability({
  archived = false,
  backendReady,
  connectionStatus,
  inputPending,
  preparationBlocked,
  taskStatus,
}: {
  archived?: boolean;
  backendReady: boolean;
  connectionStatus?: "connecting" | "ready" | "reconnecting" | "unavailable";
  inputPending: boolean;
  preparationBlocked: boolean;
  taskStatus: TaskStatus;
}) {
  const turnBusy = taskStatus === "active" || taskStatus === "blocked";
  const keepingDraftAvailable = !backendReady
    && (connectionStatus === "reconnecting" || connectionStatus === "unavailable");
  const editingDisabled = archived || (!backendReady && !keepingDraftAvailable) || inputPending || preparationBlocked;
  const sendDisabled = archived || !backendReady || inputPending || preparationBlocked;
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
  if (taskStatus === "blocked") {
    return { editingDisabled, sendDisabled, placeholder: "Draft follow-up while input is pending." };
  }
  if (turnBusy) return { editingDisabled, sendDisabled, placeholder: "Send a follow-up" };
  return { editingDisabled, sendDisabled, placeholder: "Send follow-up" };
}

export function initialTaskScrollTop(savedScrollTop: number | undefined, scrollHeight: number) {
  return savedScrollTop ?? scrollHeight;
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

export function chatFollowModeForPosition({
  clientHeight,
  previousScrollTop,
  scrollHeight,
  scrollTop,
}: {
  clientHeight: number;
  previousScrollTop?: number;
  scrollHeight: number;
  scrollTop: number;
}) {
  const distanceFromBottom = scrollHeight - (scrollTop + clientHeight);
  if (previousScrollTop !== undefined && scrollTop < previousScrollTop) return false;
  return distanceFromBottom <= 48;
}

export function scrollTopForGeneratedContent({
  followMode,
  generating,
  scrollHeight,
}: {
  followMode: boolean;
  generating: boolean;
  scrollHeight: number;
}) {
  if (!followMode || !generating) return undefined;
  return scrollHeight;
}
