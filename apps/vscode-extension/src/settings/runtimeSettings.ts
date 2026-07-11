import {
  SETTINGS_GET_RUNTIME,
  type RuntimeSettingsResult as ProtocolRuntimeSettingsResult,
} from "@openaide/app-server-client";
import type { RuntimeSettingsResult } from "@openaide/app-shell-contracts";
import type { RuntimeClient } from "../runtime/rpcClient";

export async function collectRuntimeSettings(
  runtime: Pick<RuntimeClient, "appServerRequest">,
): Promise<RuntimeSettingsResult> {
  return toShellRuntimeSettings(await runtime.appServerRequest(SETTINGS_GET_RUNTIME, {}));
}

export function toShellRuntimeSettings(result: ProtocolRuntimeSettingsResult): RuntimeSettingsResult {
  return {
    developer: {
      acp_trace: {
        enabled: result.developer.acpTrace.enabled,
        directory: result.developer.acpTrace.directory,
      },
    },
  };
}
