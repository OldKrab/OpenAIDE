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

/** Keeps resolvers alive for retained drafts until acceptance, removal, or Frontend disposal. */
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
    owner.current?.reconcile(frame);
  }, [dispatch, frame, newTaskId, state.appServerStateRootId, state.taskInputs, taskSurfaceMounted]);
  useLayoutEffect(() => () => {
    owner.current?.dispose(latestFrame.current);
  }, []);

  return owner.current;
}
