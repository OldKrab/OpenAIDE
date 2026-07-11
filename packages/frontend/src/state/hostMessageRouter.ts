import type { HostToWebviewMessage } from "@openaide/app-shell-contracts";
import { routeNavigationMessage } from "./hostNavigationMessages";
import { routeRuntimeError } from "./hostRuntimeErrorMessages";
import { routeSettingsMessage } from "./hostSettingsMessages";
import { routeTaskMessage } from "./hostTaskMessages";
import type { HostMessageRouterContext } from "./hostMessageRouterTypes";

export type { HostMessageRouterContext } from "./hostMessageRouterTypes";
export { sendWebviewTelemetry } from "./hostMessageTelemetry";

export function routeHostMessage(message: HostToWebviewMessage, context: HostMessageRouterContext) {
  if (routeSettingsMessage(message, context)) return;
  if (routeNavigationMessage(message, context)) return;
  if (routeTaskMessage(message, context)) return;
  routeRuntimeError(message, context);
}
