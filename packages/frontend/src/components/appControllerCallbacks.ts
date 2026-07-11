import type { AppControllerCallbacks, AppCallbacksDependencies } from "./appControllerCallbackTypes";
import { createNavigationCallbacks } from "./navigationCallbacks";
import { createNewTaskCallbacks } from "./newTaskCallbacks";
import { createSettingsCallbacks } from "./settingsCallbacks";
import { createTaskCallbacks } from "./taskCallbacks";

export type {
  AppControllerCallbacks,
  NavigationCallbacks,
  NewTaskCallbacks,
  SettingsCallbacks,
  TaskCallbacks,
} from "./appControllerCallbackTypes";

export function createAppCallbacks({
  acceptTaskOpen,
  backendConnection,
  beginNavigationChange,
  createSnapshotRequestId,
  currentNavigationGeneration,
  dispatch,
  latestOptionsRequestKey,
  newTaskStartAttempt,
  pendingPreparedNewTask,
  requestNativeSessions,
  setAgents,
  setPreferences,
  state,
}: AppCallbacksDependencies): AppControllerCallbacks {
  return {
    navigation: createNavigationCallbacks({
      acceptTaskOpen,
      backendConnection,
      beginNavigationChange,
      createSnapshotRequestId,
      currentNavigationGeneration,
      dispatch,
      requestNativeSessions,
      state,
    }),
    newTask: createNewTaskCallbacks({
      backendConnection,
      currentNavigationGeneration,
      dispatch,
      latestOptionsRequestKey,
      newTaskStartAttempt,
      pendingPreparedNewTask,
      state,
    }),
    settings: createSettingsCallbacks({ backendConnection, dispatch, setAgents, setPreferences, state }),
    task: createTaskCallbacks({ backendConnection, createSnapshotRequestId, dispatch, state }),
  };
}
