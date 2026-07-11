import type { ChatMessage } from "@openaide/app-shell-contracts";

export function visibleNormalizedChatItems(items: ChatMessage[]) {
  return items.filter(isVisibleChatItem).map(normalizeLegacyThoughtItem);
}

function isVisibleChatItem(item: ChatMessage) {
  const message = item.message;
  if (message.kind !== "activity") return true;
  if (isLegacySessionCatalogActivity(message)) return false;
  if (message.title !== "Working") return true;
  return !(
    message.steps.length === 1 &&
    message.steps[0].kind === "text" &&
    (message.steps[0].text === "Started" || message.steps[0].text === "Working")
  );
}

function isLegacySessionCatalogActivity(message: Extract<ChatMessage["message"], { kind: "activity" }>) {
  if (message.steps.length !== 1 || message.steps[0].kind !== "text") return false;
  return (
    (message.title === "Updated slash commands" && message.steps[0].text === "Slash commands changed.") ||
    (message.title === "Updated session options" && message.steps[0].text === "Session options changed.")
  );
}

function normalizeLegacyThoughtItem(item: ChatMessage): ChatMessage {
  const message = item.message;
  if (message.kind !== "activity" || !isLegacyThoughtActivity(message)) return item;
  const step = message.steps[0];
  const text = step.kind === "tool" ? step.output_preview ?? "" : "";
  return {
    ...item,
    message_type: "thought",
    message: {
      kind: "thought",
      id: message.id,
      text,
      created_at: message.created_at,
      streaming: message.status === "running",
    },
  };
}

function isLegacyThoughtActivity(message: Extract<ChatMessage["message"], { kind: "activity" }>) {
  return (
    message.title === "Thought" &&
    message.steps.length === 1 &&
    message.steps[0].kind === "tool" &&
    message.steps[0].name === "think"
  );
}
