import type { TaskSnapshot } from "@openaide/app-shell-contracts";
import type { AppState } from "../state/store";

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
