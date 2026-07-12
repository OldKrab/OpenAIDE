import {
  TASK_OPEN,
  type BackendConnection,
  type TaskId,
} from "@openaide/app-server-client";
import { mapProtocolTaskSnapshot } from "../state/appServerProtocolMapping";
import type { AppAction } from "../state/appReducer";

/** Re-reads one Task after a failed mutation may have changed durable backend state. */
export async function refreshTaskSnapshotAfterMutationFailure({
  dispatch,
  request,
  taskId,
}: {
  dispatch: (action: AppAction) => void;
  request: BackendConnection["request"];
  taskId: string;
}) {
  try {
    const result = await request(TASK_OPEN, { taskId: taskId as TaskId });
    dispatch({
      type: "snapshot",
      snapshot: mapProtocolTaskSnapshot(result.task).snapshot,
      intent: "refresh",
    });
  } catch {
    // The original mutation error remains actionable; reconnect will retry state sync.
  }
}
