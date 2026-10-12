import type { HostToWebviewMessage } from "@openaide/app-shell-contracts";
import type { WorkspaceRoot } from "./composerOptions";
import type { HostMessageRouterContext } from "./hostMessageRouterTypes";

export function routeSettingsMessage(message: HostToWebviewMessage, context: HostMessageRouterContext) {
  switch (message.type) {
    case "workspace.roots.result":
      context.dispatch({ type: "workspace:roots", roots: message.payload.roots as WorkspaceRoot[] });
      return true;
    case "showSettings":
      context.openSettingsSurface();
      return true;
    default:
      return false;
  }
}
