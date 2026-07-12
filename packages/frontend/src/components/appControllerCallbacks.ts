import type { AppControllerCallbacks, AppCallbacksDependencies } from "./appControllerCallbackTypes";
import { createNavigationCallbacks } from "./navigationCallbacks";
import { createNewTaskCallbacks } from "./newTaskCallbacks";
import { createSettingsCallbacks } from "./settingsCallbacks";
import { createTaskCallbacks } from "./taskCallbacks";
import { PreparedTaskOwnership } from "./preparedTaskOwnership";

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
  beginNavigationChange,
  clientInstanceId,
  createChatPageRequestGeneration,
  createSnapshotRequestId,
  currentNavigationGeneration,
  currentNewTaskPreparationKey,
  dispatch,
  latestOptionsRequestKey,
  newTaskStartAttempt,
  pendingPreparedNewTask,
  preparedTaskOwnership = new PreparedTaskOwnership(),
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
      attachmentResources,
      createSnapshotRequestId,
      currentNavigationGeneration,
      dispatch,
      preparedTaskOwnership,
      requestNativeSessions,
      state,
    }),
    newTask: createNewTaskCallbacks({
      attachmentResources,
      backendConnection,
      beginNavigationChange,
      clientInstanceId,
      currentNavigationGeneration,
      currentNewTaskPreparationKey,
      dispatch,
      latestOptionsRequestKey,
      newTaskStartAttempt,
      pendingPreparedNewTask,
      preparedTaskOwnership,
      state,
    }),
    settings: createSettingsCallbacks({ backendConnection, dispatch, setAgents, setPreferences, state }),
    task: createTaskCallbacks({
      attachmentResources,
      backendConnection,
      clientInstanceId,
      createChatPageRequestGeneration,
      createSnapshotRequestId,
      dispatch,
      state,
    }),
  };
}
