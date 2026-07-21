import {
  openNewTaskSurface,
  openNativeSessionSurface,
  openSettingsSurface,
  openTaskSurface,
} from "../services/hostBridge";
import {
  AGENT_PROBE,
  TASK_NAVIGATION_LOAD_MORE,
  TASK_NAVIGATION_REFRESH,
  type AgentId,
  type ProjectId,
} from "@openaide/app-server-client";
import { applyProtocolAgents } from "../state/appServerAgents";
import { requestTaskList, requestTaskSetArchived } from "../intents/taskReadIntents";
import { newTaskPreparationKey } from "../state/newTaskPreparationContext";
import type { AppCallbacksDependencies, NavigationCallbacks } from "./appControllerCallbackTypes";
import {
  newTaskNavigationTarget,
  nativeSessionNavigationTarget,
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
  | "attachmentResources"
  | "backendConnection"
  | "asyncOperations"
  | "dispatch"
  | "setAgents"
  | "state"
> & { newTaskController: NewTaskController };

export function createNavigationCallbacks({
  attachmentResources,
  backendConnection,
  asyncOperations,
  dispatch,
  newTaskController,
  setAgents,
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
          asyncOperations.beginNavigation(newTaskNavigationTarget(archivedProjectId));
          openNewTaskSurface(archivedProjectId);
        }
        const request = backendConnection.request;
        // The focused Task Navigation event, not the mutation response, updates the sidebar.
        void requestTaskSetArchived(
          { backendConnection: { request }, dispatch },
          taskId,
          true,
        ).catch((error) => dispatch({
          type: "tasks:error",
          message: error instanceof Error ? error.message : "Unable to archive task.",
        }));
        return;
      }
      dispatch({ type: "tasks:error", message: "App Server connection unavailable." });
    },
    changeSearch: (query) => dispatch({ type: "search:set", query }),
    loadNativeSessions: (cursor, projectId, targetRowCount) => {
      if (backendConnection?.request && projectId && targetRowCount !== undefined) {
        void backendConnection.request(TASK_NAVIGATION_LOAD_MORE, {
          projectId: projectId as ProjectId,
          targetRowCount,
        });
        return;
      }
      if (backendConnection?.request) {
        void backendConnection.request(TASK_NAVIGATION_REFRESH, {});
        return;
      }
      dispatch({ type: "tasks:error", message: "App Server connection unavailable." });
    },
    openNativeSession: (session) => {
      // A pending adoption blocks only its own row; navigating elsewhere supersedes its surface.
      if (state.newTask.submitting && state.newTask.nativeSessions.adoptingSessionId === undefined) return;
      const agentId = session.agent_id ?? state.newTask.selection.agentId;
      asyncOperations.beginNavigation(nativeSessionNavigationTarget(agentId, session.session_id));
      openNativeSessionSurface(agentId, session.session_id, session.project_id);
    },
    openNewTask: (projectId) => {
      asyncOperations.beginNavigation(newTaskNavigationTarget(projectId));
      openNewTaskSurface(projectId);
    },
    openSettings: (agentId, returnToNewTask, projectId) => {
      asyncOperations.beginNavigation(settingsNavigationTarget());
      openSettingsSurface(agentId, returnToNewTask, projectId);
    },
    retryAgent: async (agentId) => {
      if (!backendConnection?.request) return false;
      try {
        const result = await backendConnection.request(AGENT_PROBE, { agentId: agentId as AgentId });
        applyProtocolAgents(result.agents, state.newTask.selection.agentId, setAgents ?? (() => undefined), dispatch);
        const ready = result.agents.agents.some((agent) => agent.agentId === agentId && agent.status === "connected");
        if (!ready) return false;
        const taskId = newTaskController.currentTaskId();
        if (taskId) {
          await newTaskController.discard({
            attachmentResources,
            dispatch,
            lease: newTaskController.currentLease(taskId),
            request: backendConnection.request,
            taskId,
          });
        }
        newTaskController.retryPreparation();
        return true;
      } catch {
        return false;
      }
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

export function discardPreparedNewTask({
  attachmentResources,
  backendConnection,
  dispatch,
  newTaskController,
  state,
}: Pick<NavigationDependencies, "attachmentResources" | "backendConnection" | "dispatch" | "newTaskController" | "state">) {
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
}
