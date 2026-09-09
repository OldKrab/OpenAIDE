import { TASK_RESOLVE_CONFIG_PREFERENCES, type BackendConnection, type ConfigPreferencesResolution, type TaskId } from "@openaide/app-server-client";
import type { AppAction } from "../state/appReducer";
import { mapProtocolTaskSnapshot } from "../state/appServerProtocolMapping";

/** Explicit recovery mutation: never resend automatically after an unknown transport outcome. */
export async function resolveConfigPreferencesIntent(
  dependencies: { request: BackendConnection["request"]; dispatch: (action: AppAction) => void },
  taskId: TaskId,
  action: ConfigPreferencesResolution,
) {
  const result = await dependencies.request(TASK_RESOLVE_CONFIG_PREFERENCES, { taskId, action });
  dependencies.dispatch({ type: "snapshot", snapshot: mapProtocolTaskSnapshot(result.task).snapshot, intent: "refresh" });
}
