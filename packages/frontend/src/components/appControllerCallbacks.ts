import type { AppControllerCallbacks, AppCallbacksDependencies } from "./appControllerCallbackTypes";
import { createNavigationCallbacks } from "./navigationCallbacks";
import { createNewTaskCallbacks } from "./newTaskCallbacks";
import { createSettingsCallbacks } from "./settingsCallbacks";
import { createTaskCallbacks } from "./taskCallbacks";
import { NewTaskController } from "./newTaskController";

export type {
  AppControllerCallbacks,
  NavigationCallbacks,
  NewTaskCallbacks,
  SettingsCallbacks,
  TaskCallbacks,
} from "./appControllerCallbackTypes";

export function createAppCallbacks({
  acceptTaskOpen,
  attachmentResources,
  backendConnection,
  asyncOperations,
  clientInstanceId,
  createSnapshotRequestId,
  dispatch,
  newTaskStartAttempt,
  pendingPreparedNewTask,
  newTaskController = new NewTaskController(),
  setAgents,
  setPreferences,
  state,
}: AppCallbacksDependencies): AppControllerCallbacks {
  return {
    navigation: createNavigationCallbacks({
      backendConnection,
      asyncOperations,
      attachmentResources,
      dispatch,
      newTaskController,
      setAgents,
      state,
    }),
    newTask: createNewTaskCallbacks({
      attachmentResources,
      backendConnection,
      asyncOperations,
      clientInstanceId,
      dispatch,
      newTaskStartAttempt,
      pendingPreparedNewTask,
      newTaskController,
      state,
    }),
    settings: createSettingsCallbacks({ backendConnection, dispatch, setAgents, setPreferences, state }),
    task: createTaskCallbacks({
      attachmentResources,
      backendConnection,
      clientInstanceId,
      asyncOperations,
      createSnapshotRequestId,
      dispatch,
      state,
    }),
  };
}
