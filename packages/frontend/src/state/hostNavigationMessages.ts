import type { HostToWebviewMessage } from "@openaide/app-shell-contracts";
import type { HostMessageRouterContext } from "./hostMessageRouterTypes";

export function routeNavigationMessage(message: HostToWebviewMessage, context: HostMessageRouterContext) {
  switch (message.type) {
    case "surface.focusChanged":
      if (context.bootstrap.surface === "navigation") {
        context.setNavigationFocusedTaskId(message.payload.task_id ?? null);
      }
      return true;
    case "newTask":
      context.openNewTaskSurface();
      return true;
    default:
      return false;
  }
}
