import type { WebviewToHostMessage } from "@openaide/app-shell-contracts";
import { sanitizeDiagnosticText } from "../logging/logger";
import type { MessageContext } from "./messagingContext";
import { isObject, webviewActionFields, webviewTelemetryFields } from "./messagingFields";
import { routeHostCapabilityCommand, routeSurfaceCommand } from "./messagingShell";
import { routeDiagnosticsCommand, routeSettingsCommand } from "./messagingSettings";

export async function handleWebviewMessage(message: unknown, context: MessageContext) {
  if (!isObject(message) || typeof message.type !== "string") return;
  const fields = webviewActionFields(message);
  context.logger.info("webview action received", fields);

  try {
    await routeWebviewMessage(message as WebviewToHostMessage, context);
    context.logger.info("webview action completed", fields);
  } catch (error) {
    const safeMessage = sanitizeDiagnosticText(error);
    context.logger.warn("webview action failed", { ...fields, error: safeMessage });
    await context.post({
      type: "runtime.error",
      payload: {
        message: safeMessage,
        task_id:
          isObject(message.payload) && typeof message.payload.task_id === "string"
            ? message.payload.task_id
            : undefined,
        action: message.type as WebviewToHostMessage["type"],
        options_request_key: typeof message.options_request_key === "string" ? message.options_request_key : undefined,
        session_list_request_id:
          typeof message.session_list_request_id === "number" ? message.session_list_request_id : undefined,
        session_list_request_key:
          typeof message.session_list_request_key === "string" ? message.session_list_request_key : undefined,
      },
    });
  }
}

async function routeWebviewMessage(message: WebviewToHostMessage, context: MessageContext) {
  if (await routeRuntimeCommand(message, context)) return;
  if (await routeDiagnosticsCommand(message, context)) return;
  if (await routeSettingsCommand(message, context)) return;
  if (await routeSurfaceCommand(message, context)) return;
  if (await routeHostCapabilityCommand(message, context)) return;
}

async function routeRuntimeCommand(message: WebviewToHostMessage, context: MessageContext) {
  if (message.type === "webview.telemetry" && isObject(message.payload)) {
    context.logger.info("webview telemetry", webviewTelemetryFields(message.payload));
    return true;
  }
  return false;
}
