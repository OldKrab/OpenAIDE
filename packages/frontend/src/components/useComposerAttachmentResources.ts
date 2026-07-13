import { useLayoutEffect, useRef, type Dispatch } from "react";
import type { BackendConnection, ClientInstanceId } from "@openaide/app-server-client";
import {
  ComposerAttachmentResourceOwner,
  attachmentHandleResource,
  composerAttachmentResourceFrame,
  releaseAttachmentResources,
} from "../services/attachmentResources";
import type { AppAction } from "../state/appReducer";
import type { AppState } from "../state/store";

type ComposerAttachmentResourceOptions = {
  backendConnection?: Partial<Pick<BackendConnection, "request">>;
  clientInstanceId: ClientInstanceId | string;
  dispatch?: Dispatch<AppAction>;
  newTaskId?: string;
  state: AppState;
  taskSurfaceMounted: boolean;
};

/** Keeps App Server resolver lifetime aligned with the mounted Frontend composer. */
export function useComposerAttachmentResources({
  backendConnection,
  clientInstanceId,
  dispatch,
  newTaskId,
  state,
  taskSurfaceMounted,
}: ComposerAttachmentResourceOptions) {
  const latest = useRef({ backendConnection });
  latest.current = { backendConnection };
  const frame = composerAttachmentResourceFrame(state, taskSurfaceMounted, newTaskId);
  const latestFrame = useRef(frame);
  latestFrame.current = frame;
  const previousStateRootId = useRef(state.appServerStateRootId);
  const previousTaskSurfaceMounted = useRef(taskSurfaceMounted);
  const owner = useRef<ComposerAttachmentResourceOwner | undefined>(undefined);
  if (!owner.current) {
    owner.current = new ComposerAttachmentResourceOwner({
      release: (taskId, handleIds) => {
        releaseAttachmentResources(
          latest.current.backendConnection,
          taskId,
          handleIds.map(attachmentHandleResource),
        );
      },
    });
  }

  useLayoutEffect(() => {
    if (previousStateRootId.current !== state.appServerStateRootId) {
      // Resolver ids are meaningful only inside their creating state root. Once
      // the connection switches roots, the old resources cannot be released safely.
      owner.current?.replaceStateRoot();
      previousStateRootId.current = state.appServerStateRootId;
    }
    const taskSurfaceUnmounted = previousTaskSurfaceMounted.current && !taskSurfaceMounted;
    previousTaskSurfaceMounted.current = taskSurfaceMounted;
    if (taskSurfaceUnmounted && dispatch) {
      // The owner releases resolver resources below. Remove their rows in the
      // same commit so a later route cannot render a handle that was released.
      for (const [taskId, input] of Object.entries(state.taskInputs)) {
        if (taskId === newTaskId) continue;
        for (const attachment of input.context) {
          if (!attachment.app_server_handle_id) continue;
          dispatch({
            type: "taskInput:attachment:remove",
            taskId,
            attachmentId: attachment.local_id,
          });
        }
      }
    }
    owner.current?.reconcile(frame);
  }, [dispatch, frame, newTaskId, state.appServerStateRootId, state.taskInputs, taskSurfaceMounted]);
  useLayoutEffect(() => () => {
    owner.current?.dispose(latestFrame.current);
  }, []);

  return owner.current;
}
