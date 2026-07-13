import type { ChatMessage } from "@openaide/app-shell-contracts";

export function visibleNormalizedChatItems(items: ChatMessage[]) {
  return items.filter(isVisibleChatItem);
}

function isVisibleChatItem(item: ChatMessage) {
  const message = item.message;
  if (message.kind !== "activity") return true;
  if (message.title !== "Working") return true;
  return !(
    message.steps.length === 1 &&
    message.steps[0].kind === "text" &&
    (message.steps[0].text === "Started" || message.steps[0].text === "Working")
  );
}
