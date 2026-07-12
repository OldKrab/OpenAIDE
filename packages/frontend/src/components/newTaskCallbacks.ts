import {
  TASK_SET_CONFIG_OPTION,
  type AgentConfigOptionId,
  type ClientMutationId,
  type TaskId,
} from "@openaide/app-server-client";
import {
  attachmentHandleResource,
  releaseAttachmentResources,
} from "../services/attachmentResources";
import { mapProtocolTaskSnapshot } from "../state/appServerProtocolMapping";
import { newTaskPreparationKey } from "../state/newTaskPreparationContext";
import type { AppCallbacksDependencies, NewTaskCallbacks } from "./appControllerCallbackTypes";
import { createNewTaskBrowserCallbacks } from "./newTaskBrowserCallbacks";
import { createNewTaskStartCallbacks } from "./newTaskStartCallbacks";
import type { NewTaskController } from "./newTaskController";
import { refreshTaskSnapshotAfterMutationFailure } from "./taskSnapshotRefresh";

type NewTaskDependencies = Pick<
  AppCallbacksDependencies,
  | "attachmentResources"
  | "backendConnection"
  | "beginNavigationChange"
  | "clientInstanceId"
  | "currentNavigationGeneration"
  | "currentNewTaskPreparationKey"
  | "dispatch"
  | "latestOptionsRequestKey"
  | "newTaskStartAttempt"
  | "pendingPreparedNewTask"
  | "state"
> & { newTaskController: NewTaskController };

/** Composes deep New Task start and browser workflows with the remaining immediate mutations. */
export function createNewTaskCallbacks(dependencies: NewTaskDependencies): NewTaskCallbacks {
  const {
    attachmentResources,
    backendConnection,
    currentNavigationGeneration,
    currentNewTaskPreparationKey,
    dispatch,
    latestOptionsRequestKey,
    state,
  } = dependencies;
  return {
    ...createNewTaskStartCallbacks(dependencies),
    ...createNewTaskBrowserCallbacks(dependencies),
    removeAttachment: (attachmentId) => {
      const taskId = state.snapshot && !state.snapshot.task.has_messages
        ? state.snapshot.task.task_id
        : undefined;
      if (!taskId) {
        dispatch({ type: "newTask:attachment:remove", attachmentId });
        return;
      }
      const attachment = state.taskInputs[taskId]?.context.find((item) => item.local_id === attachmentId);
      dispatch({ type: "taskInput:attachment:remove", taskId, attachmentId });
      if (attachment?.app_server_handle_id && attachmentResources) {
        attachmentResources.release({ taskId, handleId: attachment.app_server_handle_id });
        return;
      }
      releaseAttachmentResources(
        backendConnection,
        taskId,
        attachment?.app_server_handle_id
          ? [attachmentHandleResource(attachment.app_server_handle_id)]
          : [],
      );
    },
    resetOptionsRequestKey: () => {
      latestOptionsRequestKey.current = undefined;
    },
    selectConfigOption: (configId, value) => {
      const mutationNavigationGeneration = currentNavigationGeneration();
      const mutationPreparationKey = newTaskPreparationKey(state);
      const taskId = state.snapshot && !state.snapshot.task.has_messages
        ? state.snapshot.task.task_id
        : undefined;
      const request = backendConnection?.request;
      if (request && taskId) {
        void request(TASK_SET_CONFIG_OPTION, {
          taskId: taskId as TaskId,
          configId: configId as AgentConfigOptionId,
          value,
          clientMutationId: createNewTaskMutationId(configId),
        }).then((result) => {
          dispatch({ type: "snapshot", snapshot: mapProtocolTaskSnapshot(result.task).snapshot, intent: "refresh" });
        }).catch(() => {
          dispatch({ type: "newTask:configOptions:error", message: "Unable to update Agent option." });
          void refreshTaskSnapshotAfterMutationFailure({
            dispatch,
            request,
            taskId,
          }).then(() => {
            if (
              currentNavigationGeneration() !== mutationNavigationGeneration
              || currentNewTaskPreparationKey() !== mutationPreparationKey
            ) return;
            // Snapshot ingestion clears transient new-task errors. Reassert only
            // the failure that still owns this exact preparation context.
            dispatch({ type: "newTask:configOptions:error", message: "Unable to update Agent option." });
          });
        });
        return;
      }
      dispatch({ type: "newTask:configOptions:error", message: "Task session is not ready yet." });
    },
  };
}

let nextNewTaskMutationId = 1;

function createNewTaskMutationId(configId: string): ClientMutationId {
  const id = `frontend-new-task-${configId}-${nextNewTaskMutationId}`;
  nextNewTaskMutationId += 1;
  return id as ClientMutationId;
}
