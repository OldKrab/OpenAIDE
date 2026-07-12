import { useEffect, type Dispatch, type RefObject } from "react";
import type { BackendConnection, TaskId } from "@openaide/app-server-client";
import {
  startAppServerStateSubscription,
  type StateSubscriptionMappingContext,
} from "../services/appServerStateSubscriptions";
import type { AppAction } from "../state/appReducer";
import type { NewTaskController } from "./newTaskController";

type NewTaskSubscriptionOptions = {
  backendConnection?: Pick<BackendConnection, "request"> & Partial<Pick<BackendConnection, "events">>;
  backendInitialized: RefObject<boolean>;
  backendReady: boolean;
  backendStateGeneration: number;
  context: RefObject<StateSubscriptionMappingContext | undefined>;
  dispatch: Dispatch<AppAction>;
  newTaskController: NewTaskController;
  newTaskId?: string;
};

/** Keeps the client-private New Task current independently from the visible Task route. */
export function useNewTaskSubscription({
  backendConnection,
  backendInitialized,
  backendReady,
  backendStateGeneration,
  context,
  dispatch,
  newTaskController,
  newTaskId,
}: NewTaskSubscriptionOptions) {
  useEffect(() => {
    if (!backendConnection?.events || !backendReady || !backendInitialized.current || !newTaskId) return;
    const mappingContext = context.current;
    if (!mappingContext) return;
    return startAppServerStateSubscription({
      backendConnection: {
        events: backendConnection.events,
        request: backendConnection.request,
      },
      context: mappingContext,
      dispatch: (action) => {
        if (action.type === "snapshot" && action.snapshot.lifecycle === "new") {
          newTaskController.updateSnapshot(action.snapshot);
          return;
        }
        if (action.type === "snapshot" && action.snapshot.task.task_id === newTaskId) {
          newTaskController.confirmSentTask(newTaskId);
        }
        dispatch(action);
      },
      scope: { kind: "task", taskId: newTaskId as TaskId },
    });
  }, [
    backendConnection,
    backendInitialized,
    backendReady,
    backendStateGeneration,
    context,
    dispatch,
    newTaskController,
    newTaskId,
  ]);
}
