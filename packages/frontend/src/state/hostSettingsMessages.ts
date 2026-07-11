import type { HostToWebviewMessage } from "@openaide/app-shell-contracts";
import type { WorkspaceRoot } from "./composerOptions";
import type { HostMessageRouterContext } from "./hostMessageRouterTypes";

export function routeSettingsMessage(message: HostToWebviewMessage, context: HostMessageRouterContext) {
  switch (message.type) {
    case "workspace.roots.result":
      context.dispatch({ type: "workspace:roots", roots: message.payload.roots as WorkspaceRoot[] });
      return true;
    case "runtime.settings.result":
      context.dispatch({ type: "settings:runtimeSettings", settings: message.payload });
      return true;
    case "showSettings":
      context.postHostMessage({ type: "surface.openSettings" });
      return true;
    default:
      return false;
  }
}
