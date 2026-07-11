import { postHostMessage } from "../services/hostBridge";
import {
  TASK_ADOPT_NATIVE_SESSION,
  type AgentId,
  type ProjectId,
} from "@openaide/app-server-client";
import { requestTaskList, requestTaskOpen, requestTaskSetArchived } from "../intents/taskReadIntents";
import { mapProtocolTaskSnapshot } from "../state/appServerProtocolMapping";
import type { AppCallbacksDependencies, NavigationCallbacks } from "./appControllerCallbackTypes";

type NavigationDependencies = Pick<
  AppCallbacksDependencies,
  | "acceptTaskOpen"
  | "backendConnection"
  | "beginNavigationChange"
  | "createSnapshotRequestId"
  | "currentNavigationGeneration"
  | "dispatch"
  | "requestNativeSessions"
  | "state"
>;

export function createNavigationCallbacks({
  acceptTaskOpen,
  backendConnection,
  beginNavigationChange,
  createSnapshotRequestId,
  currentNavigationGeneration,
  dispatch,
  requestNativeSessions,
  state,
}: NavigationDependencies): NavigationCallbacks {
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
          postHostMessage(archivedProjectId
            ? { type: "surface.openNewTask", payload: { project_id: archivedProjectId } }
            : { type: "surface.openNewTask" });
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
      requestNativeSessions(cursor, cursor !== undefined);
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
      const navigationGeneration = beginNavigationChange();
      dispatch({ type: "newTask:nativeSessions:adopt", sessionId: session.session_id });
      if (backendConnection?.request) {
        const projectId = state.newTask.selection.projectId;
        if (!projectId) {
          dispatch({
            type: "newTask:nativeSessions:error",
            message: "Project unavailable. Refresh and try again.",
          });
          return;
        }
        void backendConnection.request(TASK_ADOPT_NATIVE_SESSION, {
          projectId: projectId as ProjectId,
          agentId: state.newTask.selection.agentId as AgentId,
          nativeSessionId: session.session_id,
          title: session.title,
        }).then((result) => {
          if (currentNavigationGeneration() !== navigationGeneration) {
            dispatch({ type: "newTask:nativeSessions:remove", sessionId: session.session_id });
            return;
          }
          const snapshot = mapProtocolTaskSnapshot(result.task).snapshot;
          dispatch({ type: "snapshot", snapshot, intent: "open" });
          dispatch({ type: "newTask:nativeSessions:remove", sessionId: session.session_id });
          postHostMessage({
            type: "surface.openTask",
            payload: {
              task_id: snapshot.task.task_id,
              title: snapshot.task.title,
            },
          });
        }).catch((error) => {
          if (currentNavigationGeneration() !== navigationGeneration) return;
          dispatch({
            type: "newTask:nativeSessions:error",
            message: error instanceof Error ? error.message : "Unable to open task.",
          });
        });
        return;
      }
      dispatch({
        type: "newTask:nativeSessions:error",
        message: "App Server connection unavailable.",
      });
    },
    openNewTask: (projectId) => {
      beginNavigationChange();
      postHostMessage(projectId
        ? { type: "surface.openNewTask", payload: { project_id: projectId } }
        : { type: "surface.openNewTask" });
    },
    openSettings: () => {
      beginNavigationChange();
      postHostMessage({ type: "surface.openSettings" });
    },
    openTask: (taskId) => {
      beginNavigationChange();
      const task = state.tasks.find((item) => item.task_id === taskId);
      dispatch({ type: "selection:set", taskId });
      if (backendConnection?.request) {
        void requestTaskOpen(
          {
            acceptTaskOpen,
            backendConnection: { request: backendConnection.request },
            createTaskOpenRequestId: createSnapshotRequestId,
            dispatch,
          },
          taskId,
          "open",
        ).catch((error) => dispatch({
          type: "taskOpen:error",
          taskId,
          message: error instanceof Error ? error.message : "Unable to open task from App Server",
        }));
      }
      postHostMessage({ type: "surface.openTask", payload: { task_id: taskId, title: task?.title } });
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
          beginNavigationChange(false);
          dispatch({ type: "archive:set", showArchived: false });
          postHostMessage({ type: "surface.openTask", payload: { task_id: taskId } });
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
      beginNavigationChange(showArchived);
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
