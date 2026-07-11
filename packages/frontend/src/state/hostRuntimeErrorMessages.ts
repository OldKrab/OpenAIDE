import type { HostToWebviewMessage } from "@openaide/app-shell-contracts";
import type { HostMessageRouterContext } from "./hostMessageRouterTypes";

export function routeRuntimeError(message: HostToWebviewMessage, context: HostMessageRouterContext) {
  if (message.type !== "runtime.error") return false;
  const payload = message.payload;
  if (payload.request_id !== undefined) {
    context.dispatch({
      type: "permission:error",
      requestId: payload.request_id,
      message: payload.message ?? "Permission response failed",
    });
    return true;
  }
  if (payload.task_id !== undefined) {
    routeTaskRuntimeError(payload, context);
    return true;
  }
  if (settingsErrorActions.has(payload.action)) {
    context.dispatch({ type: "settings:error", message: payload.message ?? "Unable to load settings" });
    return true;
  }
  context.dispatch({ type: "submit:error", message: payload.message ?? "Runtime request failed" });
  return true;
}

type RuntimeErrorPayload = Extract<HostToWebviewMessage, { type: "runtime.error" }>["payload"];

function routeTaskRuntimeError(payload: RuntimeErrorPayload, context: HostMessageRouterContext) {
  if (payload.task_id === undefined) return;
  context.dispatch({ type: "taskInput:error", taskId: payload.task_id });
}

const settingsErrorActions = new Set<string>([
  "developer.settings.unlock",
  "secret.transaction.apply",
  "secret.transaction.commit",
  "secret.transaction.rollback",
]);
