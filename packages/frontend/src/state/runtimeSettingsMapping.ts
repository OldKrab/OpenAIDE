import type { RuntimeSettingsResult as ProtocolRuntimeSettingsResult } from "@openaide/app-server-client";
import type { RuntimeSettingsResult as FrontendRuntimeSettingsResult } from "@openaide/app-shell-contracts";

export function mapProtocolRuntimeSettings(settings: ProtocolRuntimeSettingsResult): FrontendRuntimeSettingsResult {
  return {
    developer: {
      acp_trace: {
        enabled: settings.developer.acpTrace.enabled,
        directory: settings.developer.acpTrace.directory,
      },
    },
  };
}
