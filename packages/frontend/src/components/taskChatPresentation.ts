import type { TaskSnapshot } from "@openaide/app-shell-contracts";
import type { AppState } from "../state/store";

/** Keeps resolved permission history beside the Tool activity it explains. */
export function chatItemsWithResolvedPermissions(items: TaskSnapshot["chat"]["items"]) {
  const permissions = items.filter((item) => item.message.kind === "permission");
  if (!permissions.length) return items;

  const placed = new Set<string>();
  const result: TaskSnapshot["chat"]["items"] = [];
  for (const item of items) {
    if (item.message.kind === "permission") continue;
    result.push(item);
    if (item.message.kind !== "activity") continue;
    const toolCallIds = activityToolCallIds(item);
    for (const permission of permissions) {
      if (permission.message.kind !== "permission") continue;
      if (placed.has(permission.message_id)) continue;
      if (!toolCallIds.includes(permission.message.tool_call.id)) continue;
      result.push(permission);
      placed.add(permission.message_id);
    }
  }
  for (const permission of permissions) {
    if (!placed.has(permission.message_id)) result.push(permission);
  }
  return result;
}

function activityToolCallIds(item: TaskSnapshot["chat"]["items"][number]) {
  if (item.message.kind !== "activity") return [];
  return item.message.steps
    .map((step) => step.kind === "tool" ? step.tool_call_id : undefined)
    .filter((toolCallId): toolCallId is string => Boolean(toolCallId));
}

export function permissionResponseForMessage(
  message: TaskSnapshot["chat"]["items"][number]["message"],
  permissionResponses: AppState["permissionResponses"],
) {
  if (message.kind !== "permission") return undefined;
  return permissionResponses[message.app_server_request_id ?? message.request_id];
}

export function questionResponseForMessage(
  message: TaskSnapshot["chat"]["items"][number]["message"],
  questionResponses: AppState["questionResponses"],
) {
  if (message.kind !== "elicitation") return undefined;
  return questionResponses[message.app_server_request_id ?? message.request_id];
}
