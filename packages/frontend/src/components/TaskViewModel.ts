import type { ChatMessage, TaskStatus } from "@openaide/app-shell-contracts";

export function chatItemsThroughPresentationBarrier(
  items: ChatMessage[],
  presentingMessageIds: ReadonlySet<string>,
) {
  const barrierIndex = items.findIndex((item) => presentingMessageIds.has(item.message_id));
  if (barrierIndex === -1) return items;
  return presentationNeedsImmediateFlush(items, presentingMessageIds)
    ? items
    : items.slice(0, barrierIndex + 1);
}

export function presentationNeedsImmediateFlush(
  items: ChatMessage[],
  presentingMessageIds: ReadonlySet<string>,
) {
  const barrierIndex = items.findIndex((item) => presentingMessageIds.has(item.message_id));
  if (barrierIndex === -1) return false;
  return items.slice(barrierIndex + 1).some((item) => (
    (item.message.kind === "permission" && item.message.state === "pending")
    || (item.message.kind === "elicitation" && item.message.state === "pending")
    || item.message.kind === "interruption"
  ));
}

export function taskComposerAvailability({
  archived = false,
  backendReady,
  inputPending,
  preparationBlocked,
  taskStatus,
}: {
  archived?: boolean;
  backendReady: boolean;
  inputPending: boolean;
  preparationBlocked: boolean;
  taskStatus: TaskStatus;
}) {
  const turnBusy = taskStatus === "active" || taskStatus === "blocked";
  const editingDisabled = archived || !backendReady || inputPending || preparationBlocked;
  const sendDisabled = editingDisabled;
  if (archived) return { editingDisabled, sendDisabled, placeholder: "Restore task to send follow-up." };
  if (preparationBlocked) return { editingDisabled, sendDisabled, placeholder: "Preparing task." };
  if (!backendReady) return { editingDisabled, sendDisabled, placeholder: "Connecting to App Server." };
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
