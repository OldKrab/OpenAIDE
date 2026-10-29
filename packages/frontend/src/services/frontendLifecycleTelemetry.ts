import type { BackendConnection } from "@openaide/app-server-client";
import { clientDiagnosticsFor, type ClientDiagnosticFields } from "../intents/clientDiagnosticsIntents";
import { sendWebviewTelemetry } from "../state/hostMessageRouter";
import { postHostMessage } from "./hostBridge";

/**
 * Records one metadata-only Frontend lifecycle event in the App Shell's telemetry and,
 * through the connection, in the App Server log as `frontend_<event>`, where it lines up
 * with the requests it waited on.
 */
export function recordFrontendLifecycleEvent(
  backendConnection: Pick<BackendConnection, "request"> | undefined,
  event: string,
  fields: ClientDiagnosticFields,
) {
  sendWebviewTelemetry(postHostMessage, event, fields);
  if (!backendConnection) return;
  clientDiagnosticsFor(backendConnection, (error) => {
    sendWebviewTelemetry(postHostMessage, "client_diagnostics_forward_failed", {
      error_name: error instanceof Error ? error.name : typeof error,
    });
  }).record(event, fields);
}
