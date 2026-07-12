import type { TaskSnapshot } from "@openaide/app-shell-contracts";
import type { AppState } from "../state/store";

export function chatItemsWithAppServerPermissions(
  items: TaskSnapshot["chat"]["items"],
  appServerPermissionRequests: AppState["appServerPermissionRequests"],
  taskId?: string,
) {
  const mappedItems = items.map((item) => {
    const requestId = pendingRequestId(item);
    return requestId ? appServerPermissionRequests[requestId]?.message ?? item : item;
  });
  const deliveredPermissions = Object.entries(appServerPermissionRequests)
    .filter(([requestId, request]) => (
      request.taskId === taskId
      && !mappedItems.some((item) => pendingRequestId(item) === requestId)
    ))
    .map(([, request]) => request.message);
  return placePermissionsAfterMatchingActivity(dedupePermissionItems([...mappedItems, ...deliveredPermissions]));
}

export function chatItemsWithAppServerQuestions(
  items: TaskSnapshot["chat"]["items"],
  appServerQuestionRequests: AppState["appServerQuestionRequests"],
  taskId?: string,
) {
  const mappedItems = items.map((item) => {
    if (item.message.kind !== "elicitation") return item;
    const requestId = item.message.app_server_request_id ?? item.message.request_id;
    return appServerQuestionRequests[requestId]?.message ?? item;
  });
  const delivered = Object.entries(appServerQuestionRequests)
    .filter(([requestId, request]) => (
      request.taskId === taskId
      && !mappedItems.some((item) => questionRequestId(item) === requestId)
    ))
    .map(([, request]) => request.message);
  return [...mappedItems, ...delivered];
}

function questionRequestId(item: TaskSnapshot["chat"]["items"][number]) {
  if (item.message.kind !== "elicitation") return undefined;
  return item.message.app_server_request_id ?? item.message.request_id;
}

function pendingRequestId(item: TaskSnapshot["chat"]["items"][number]) {
  if (item.message.kind !== "interruption") return undefined;
  if (!item.message_id.startsWith("pending-")) return undefined;
  return item.message_id.slice("pending-".length);
}

function dedupePermissionItems(items: TaskSnapshot["chat"]["items"]) {
  return items.reduce<TaskSnapshot["chat"]["items"]>((deduped, item) => {
    if (item.message.kind !== "permission") return [...deduped, item];
    const itemMessage = item.message;
    const itemRequestId = itemMessage.app_server_request_id ?? itemMessage.request_id;
    const existingIndex = deduped.findIndex((candidate) => {
      const candidateMessage = candidate.message;
      return candidateMessage.kind === "permission"
        && permissionIdentityMatches(candidateMessage, itemRequestId, itemMessage);
    });
    if (existingIndex === -1) return [...deduped, item];
    const existing = deduped[existingIndex];
    if (!existing || existing.message.kind !== "permission") return deduped;
    if (!shouldReplacePermission(existing.message, item.message)) return deduped;
    return deduped.map((candidate, index) => index === existingIndex ? item : candidate);
  }, []);
}

function shouldReplacePermission(
  existing: Extract<TaskSnapshot["chat"]["items"][number]["message"], { kind: "permission" }>,
  candidate: Extract<TaskSnapshot["chat"]["items"][number]["message"], { kind: "permission" }>,
) {
  if (candidate.state === "resolved" && existing.state !== "resolved") return true;
  if (!existing.app_server_request_id && candidate.app_server_request_id && existing.state === "pending") return true;
  return false;
}

function permissionIdentityMatches(
  existing: Extract<TaskSnapshot["chat"]["items"][number]["message"], { kind: "permission" }>,
  requestId: string,
  permission: Extract<TaskSnapshot["chat"]["items"][number]["message"], { kind: "permission" }>,
) {
  if (existing.request_id === permission.request_id) return true;
  if (existing.request_id === requestId) return true;
  if (existing.app_server_request_id !== permission.app_server_request_id) return false;
  if (!existing.app_server_request_id) return false;
  return existing.tool_call.id === permission.tool_call.id;
}

function placePermissionsAfterMatchingActivity(items: TaskSnapshot["chat"]["items"]) {
  const activityToolIds = new Set(
    items.flatMap((item) => item.message.kind === "activity" ? activityToolCallIds(item) : []),
  );
  const colocatedPermissions = items.filter((item) => (
    item.message.kind === "permission" && activityToolIds.has(item.message.tool_call.id)
  ));
  const pendingPermissionToolIds = new Set(
    colocatedPermissions
      .map((item) => item.message.kind === "permission" && item.message.state === "pending" ? item.message.tool_call.id : undefined)
      .filter((toolCallId): toolCallId is string => Boolean(toolCallId)),
  );
  if (!colocatedPermissions.length) return items;
  const placedPermissionIds = new Set<string>();
  const nextItems: TaskSnapshot["chat"]["items"] = [];
  for (const item of items) {
    if (item.message.kind === "permission" && activityToolIds.has(item.message.tool_call.id)) continue;
    if (shouldReplaceActivityWithPendingPermission(item, pendingPermissionToolIds)) {
      appendMatchingPermissions(nextItems, colocatedPermissions, placedPermissionIds, activityToolCallIds(item));
      continue;
    }
    nextItems.push(item);
    if (item.message.kind !== "activity") continue;
    appendMatchingPermissions(nextItems, colocatedPermissions, placedPermissionIds, activityToolCallIds(item));
  }
  return nextItems;
}

function shouldReplaceActivityWithPendingPermission(
  item: TaskSnapshot["chat"]["items"][number],
  pendingPermissionToolIds: Set<string>,
) {
  if (item.message.kind !== "activity") return false;
  const toolCallIds = activityToolCallIds(item);
  return toolCallIds.length > 0 && toolCallIds.every((toolCallId) => pendingPermissionToolIds.has(toolCallId));
}

function appendMatchingPermissions(
  nextItems: TaskSnapshot["chat"]["items"],
  colocatedPermissions: TaskSnapshot["chat"]["items"],
  placedPermissionIds: Set<string>,
  toolCallIds: string[],
) {
  for (const permission of colocatedPermissions) {
    if (permission.message.kind !== "permission") continue;
    if (placedPermissionIds.has(permission.message_id)) continue;
    if (!toolCallIds.includes(permission.message.tool_call.id)) continue;
    nextItems.push(permission);
    placedPermissionIds.add(permission.message_id);
  }
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
