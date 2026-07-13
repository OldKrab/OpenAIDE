import type { HostToWebviewMessage } from "@openaide/app-shell-contracts";
import type { HostMessageRouterContext } from "./hostMessageRouterTypes";

export function routeNavigationMessage(message: HostToWebviewMessage, context: HostMessageRouterContext) {
  switch (message.type) {
    case "newTask":
      context.openNewTaskSurface();
      return true;
    default:
      return false;
  }
}
