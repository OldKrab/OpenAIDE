import { collectDiagnostics } from "../diagnostics/snapshot";
import { exportSupportDiagnostics } from "../diagnostics/export";
import type { MessageContext } from "./messagingContext";
import type { WebviewToHostMessage } from "@openaide/app-shell-contracts";

export async function routeDiagnosticsCommand(message: WebviewToHostMessage, context: MessageContext) {
  if (message.type === "diagnostics.snapshot") {
    await context.post({
      type: "diagnostics.snapshot.result",
      payload: await collectDiagnostics(context.runtime, context.runtimeProcess),
    });
    return true;
  }
  if (message.type === "diagnostics.export") {
    await exportSupportDiagnostics(context.runtime, context.runtimeProcess, context.supportExportState);
    return true;
  }
  return false;
}
