import type { HostToWebviewMessage } from "@openaide/app-shell-contracts";
import type { HostMessageRouterContext } from "./hostMessageRouterTypes";

export function routeTaskMessage(message: HostToWebviewMessage, _context: HostMessageRouterContext) {
  switch (message.type) {
    default:
      return false;
  }
}
