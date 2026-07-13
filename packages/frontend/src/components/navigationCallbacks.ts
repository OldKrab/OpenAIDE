import {
  openNewTaskSurface,
  openSettingsSurface,
  openTaskSurface,
} from "../services/hostBridge";
import {
  TASK_ADOPT_NATIVE_SESSION,
  type AgentId,
  type ProjectId,
} from "@openaide/app-server-client";
import { requestTaskList, requestTaskOpen, requestTaskSetArchived } from "../intents/taskReadIntents";
import { mapProtocolTaskSnapshot } from "../state/appServerProtocolMapping";
import { TASK_NAVIGATION_PAGE_SIZE } from "../state/taskNavigationPolicy";
import { newTaskPreparationKey } from "../state/newTaskPreparationContext";
import type { AppCallbacksDependencies, NavigationCallbacks } from "./appControllerCallbackTypes";
import {
  newTaskNavigationTarget,
  settingsNavigationTarget,
  taskListNavigationTarget,
  taskNavigationTarget,
} from "../state/asyncOperationOwner";
import {
  disposableNewTaskControllerId,
  type NewTaskController,
} from "./newTaskController";

type NavigationDependencies = Pick<
  AppCallbacksDependencies,
  | "acceptTaskOpen"
  | "attachmentResources"
  | "backendConnection"
  | "asyncOperations"
  | "createSnapshotRequestId"
  | "dispatch"
  | "requestNativeSessions"
  | "state"
> & { newTaskController: NewTaskController };

export function createNavigationCallbacks({
  acceptTaskOpen,
  attachmentResources,
  backendConnection,
  asyncOperations,
  createSnapshotRequestId,
  dispatch,
  newTaskController,
  requestNativeSessions,
  state,
}: NavigationDependencies): NavigationCallbacks {
  const discardNewTask = () => {
    const taskId = disposableNewTaskControllerId(state, newTaskController);
    if (!taskId) return undefined;
    const preparationKey = newTaskPreparationKey(state);
    if (!preparationKey) return undefined;
    const currentLease = newTaskController.currentLease();
    if (currentLease && currentLease.taskId !== taskId) return undefined;
    const lease = currentLease ?? newTaskController.claim({
      attachmentResources,
      preparationKey,
      taskId,
    });
    return newTaskController.discard({
      attachmentResources,
      dispatch,
      lease,
      request: backendConnection?.request,
      taskId,
    });
  };
  return {
    archiveTask: (taskId) => {
      const archivedTask = state.tasks.find((task) => task.task_id === taskId);
      const archivedProjectId = archivedTask?.project_id ?? (
        state.snapshot?.task.task_id === taskId ? state.snapshot.task.project_id : undefined
      );
      const archivingActiveTask = taskId === state.activeTaskId || taskId === state.snapshot?.task.task_id;
      if (archivingActiveTask) {
        dispatch({ type: "selection:clear" });
      }
      if (backendConnection?.request) {
      if (archivingActiveTask) {
          asyncOperations.beginNavigation(newTaskNavigationTarget(archivedProjectId));
          openNewTaskSurface(archivedProjectId);
        }
        const request = backendConnection.request;
        void requestTaskSetArchived(
          { backendConnection: { request }, dispatch },
          taskId,
          true,
        ).then(() =>
          requestTaskList(
            { backendConnection: { request }, dispatch },
            state.showArchived,
          ),
        ).then(() => {
          dispatch({ type: "task:list:remove", taskId });
        }).catch((error) => dispatch({
          type: "tasks:error",
          message: error instanceof Error ? error.message : "Unable to archive task.",
        }));
        return;
      }
      dispatch({ type: "tasks:error", message: "App Server connection unavailable." });
    },
    changeSearch: (query) => dispatch({ type: "search:set", query }),
    loadNativeSessions: (cursor) => {
      requestNativeSessions(
        cursor,
        cursor !== undefined,
        cursor === undefined ? state.newTask.nativeSessions.items.length : TASK_NAVIGATION_PAGE_SIZE,
      );
      if (cursor !== undefined) return;
      const taskId = state.snapshot?.task.task_id ?? state.activeTaskId;
      if (!taskId || !backendConnection?.request) return;
      void requestTaskOpen(
        {
          acceptTaskOpen,
          backendConnection: { request: backendConnection.request },
          createTaskOpenRequestId: createSnapshotRequestId,
          dispatch,
        },
        taskId,
        "refresh",
      ).catch((error) => dispatch({
        type: "taskOpen:error",
        taskId,
        message: error instanceof Error ? error.message : "Unable to refresh task from App Server",
      }));
    },
    openNativeSession: (session) => {
      if (state.newTask.submitting) return;
      const newTaskDisposal = discardNewTask();
      asyncOperations.beginNavigation();
      const operation = asyncOperations.claim("native-session-adoption");
      dispatch({ type: "newTask:nativeSessions:adopt", sessionId: session.session_id });
      if (backendConnection?.request) {
        const request = backendConnection.request;
        const projectId = state.newTask.selection.projectId;
        if (!projectId) {
          dispatch({
            type: "newTask:nativeSessions:error",
            sessionId: session.session_id,
            message: "Workspace unavailable. Refresh and try again.",
          });
          return;
        }
        const adopt = () => request(TASK_ADOPT_NATIVE_SESSION, {
          projectId: projectId as ProjectId,
          agentId: state.newTask.selection.agentId as AgentId,
          nativeSessionId: session.session_id,
          title: session.title,
        });
        const adoption = newTaskDisposal ? newTaskDisposal.then(adopt) : adopt();
        void adoption.then((result) => {
          if (!asyncOperations.owns(operation)) {
            dispatch({ type: "newTask:nativeSessions:remove", sessionId: session.session_id });
            return;
          }
          const snapshot = mapProtocolTaskSnapshot(result.task).snapshot;
          dispatch({ type: "snapshot", snapshot, intent: "open" });
          dispatch({ type: "newTask:nativeSessions:remove", sessionId: session.session_id });
          asyncOperations.expectNavigation(taskNavigationTarget(snapshot.task.task_id));
          openTaskSurface(snapshot.task.task_id, snapshot.task.title);
        }).catch((error) => {
          if (!asyncOperations.owns(operation)) return;
          dispatch({
            type: "newTask:nativeSessions:error",
            sessionId: session.session_id,
            message: error instanceof Error ? error.message : "Unable to open task.",
          });
        });
        return;
      }
      dispatch({
        type: "newTask:nativeSessions:error",
        sessionId: session.session_id,
        message: "App Server connection unavailable.",
      });
    },
    openNewTask: (projectId) => {
      asyncOperations.beginNavigation(newTaskNavigationTarget(projectId));
      openNewTaskSurface(projectId);
    },
    openSettings: () => {
      asyncOperations.beginNavigation(settingsNavigationTarget());
      openSettingsSurface();
    },
    openTask: (taskId) => {
      asyncOperations.beginNavigation(taskNavigationTarget(taskId));
      const task = state.tasks.find((item) => item.task_id === taskId);
      dispatch({ type: "selection:set", taskId });
      openTaskSurface(taskId, task?.title);
    },
    restoreTask: (taskId) => {
      if (backendConnection?.request) {
        const request = backendConnection.request;
        void requestTaskSetArchived(
          { backendConnection: { request }, dispatch },
          taskId,
          false,
        ).then(() =>
          requestTaskList(
            { backendConnection: { request }, dispatch },
            false,
          ),
        ).then(() => {
          dispatch({ type: "taskInput:clear", taskId });
          asyncOperations.beginNavigation(taskNavigationTarget(taskId), false);
          dispatch({ type: "archive:set", showArchived: false });
          openTaskSurface(taskId);
        }).catch((error) => dispatch({
          type: "tasks:error",
          message: error instanceof Error ? error.message : "Unable to restore task.",
        }));
        return;
      }
      dispatch({ type: "tasks:error", message: "App Server connection unavailable." });
    },
    toggleArchived: () => {
      const showArchived = !state.showArchived;
      asyncOperations.beginNavigation(taskListNavigationTarget(showArchived), showArchived);
      dispatch({ type: "archive:set", showArchived });
      const cachedTasks = state.taskListCache[showArchived ? "archived" : "active"];
      if (cachedTasks !== undefined) return;
      if (backendConnection?.request) {
        void requestTaskList(
          { backendConnection: { request: backendConnection.request }, dispatch },
          showArchived,
        ).catch((error) => dispatch({
          type: "tasks:error",
          message: error instanceof Error ? error.message : "Unable to load tasks from App Server",
        }));
        return;
      }
      dispatch({ type: "tasks:error", message: "App Server connection unavailable." });
    },
  };
}
