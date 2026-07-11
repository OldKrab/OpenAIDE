import { collectDiagnostics } from "../diagnostics/snapshot";
import { openDiagnosticsDocument } from "../diagnostics/export";
import { collectRuntimeSettings } from "../settings/runtimeSettings";
import { unlockDeveloperSettings } from "../settings/snapshot";
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
    await openDiagnosticsDocument(context.runtime, context.runtimeProcess);
    return true;
  }
  return false;
}

export async function routeSettingsCommand(message: WebviewToHostMessage, context: MessageContext) {
  if (message.type === "developer.settings.unlock" && context.developerSettingsStore) {
    await unlockDeveloperSettings(context.developerSettingsStore);
    await context.post({
      type: "runtime.settings.result",
      payload: await collectRuntimeSettings(context.runtime),
    });
    return true;
  }
  return false;
}
