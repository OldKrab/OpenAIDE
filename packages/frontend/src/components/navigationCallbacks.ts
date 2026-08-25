import {
  openNewTaskSurface,
  openNativeSessionSurface,
  openSettingsSurface,
  openTaskSurface,
} from "../services/hostBridge";
import {
  AGENT_PROBE,
  NATIVE_SESSION_ARCHIVE,
  NATIVE_SESSION_FORK,
  NATIVE_SESSION_RESTORE,
  NATIVE_SESSION_SET_PINNED,
  NATIVE_SESSION_SET_TITLE,
  TASK_NAVIGATION_LOAD_MORE,
  TASK_NAVIGATION_REFRESH,
  TASK_ARCHIVE_OLDER,
  TASK_SET_PINNED,
  TASK_SET_TITLE,
  type AgentId,
  type ProjectId,
  type TaskId,
  AppServerProtocolError,
} from "@openaide/app-server-client";
import { applyProtocolAgents } from "../state/appServerAgents";
import { requestTaskArchive, requestTaskRestore } from "../intents/taskReadIntents";
import { newTaskPreparationKey } from "../state/newTaskPreparationContext";
import { nativeSessionMutationKey, taskForkMutationKey } from "../state/store";
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
  // Fork is non-idempotent and has no client mutation id. Guard synchronously
  // so repeated clicks cannot outrun the next React state publication.
  const pendingForkKeys = new Set<string>();
  return {
    archiveNativeSession: (session) => {
      mutateNativeSessionArchive("archive", session);
    },
    archiveOlderTasks: async (cutoff, preview) => {
      if (!backendConnection?.request) throw new Error("App Server connection unavailable.");
      return backendConnection.request(TASK_ARCHIVE_OLDER, {
        cutoff,
        preview,
      });
    },
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
        void requestTaskArchive(
          { backendConnection: { request }, dispatch },
          taskId,
        ).catch((error) => dispatch({
          type: "tasks:error",
          message: error instanceof Error ? error.message : "Unable to archive task.",
        }));
        return;
      }
      dispatch({ type: "tasks:error", message: "App Server connection unavailable." });
    },
    forkNativeSession: (session) => {
      const agentId = session.agent_id ?? state.newTask.selection.agentId;
      mutateNativeSessionFork(
        nativeSessionMutationKey(agentId, session.session_id),
        {
          kind: "nativeSession",
          agentId: agentId as AgentId,
          nativeSessionId: session.session_id,
        },
      );
    },
    forkTask: (taskId) => {
      mutateNativeSessionFork(taskForkMutationKey(taskId), {
        kind: "task",
        taskId: taskId as TaskId,
      });
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
    openSettings: (agentId, returnToNewTask, projectId, settingsTab) => {
      asyncOperations.beginNavigation(settingsNavigationTarget());
      openSettingsSurface(agentId, returnToNewTask, projectId, settingsTab);
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
      openTaskSurface(taskId, task?.title, task?.agent_id);
    },
    restoreTask: (taskId) => {
      if (backendConnection?.request) {
        const request = backendConnection.request;
        void requestTaskRestore(
          { backendConnection: { request }, dispatch },
          taskId,
        ).then(() => {
          dispatch({ type: "taskInput:clear", taskId });
          asyncOperations.beginNavigation(taskNavigationTarget(taskId), false);
          dispatch({ type: "archive:set", showArchived: false });
          const restoredTask = state.tasks.find((task) => task.task_id === taskId);
          openTaskSurface(taskId, undefined, restoredTask?.agent_id);
        }).catch((error) => dispatch({
          type: "tasks:error",
          message: error instanceof Error ? error.message : "Unable to restore task.",
        }));
        return;
      }
      dispatch({ type: "tasks:error", message: "App Server connection unavailable." });
    },
    setTaskTitle: async (taskId, title) => {
      if (!backendConnection?.request) {
        throw new Error("App Server connection unavailable.");
      }
      await backendConnection.request(TASK_SET_TITLE, {
        taskId: taskId as TaskId,
        title,
      });
    },
    setTaskPinned: async (taskId, pinned) => {
      if (!backendConnection?.request) {
        throw new Error("App Server connection unavailable.");
      }
      await backendConnection.request(TASK_SET_PINNED, {
        taskId: taskId as TaskId,
        pinned,
      });
    },
    restoreNativeSession: (session) => {
      mutateNativeSessionArchive("restore", session);
    },
    setNativeSessionTitle: async (session, title) => {
      if (!backendConnection?.request) throw new Error("App Server connection unavailable.");
      const agentId = session.agent_id ?? state.newTask.selection.agentId;
      await backendConnection.request(NATIVE_SESSION_SET_TITLE, {
        agentId: agentId as AgentId,
        nativeSessionId: session.session_id,
        title,
      });
    },
    setNativeSessionPinned: async (session, pinned) => {
      if (!backendConnection?.request) throw new Error("App Server connection unavailable.");
      const agentId = session.agent_id ?? state.newTask.selection.agentId;
      await backendConnection.request(NATIVE_SESSION_SET_PINNED, {
        agentId: agentId as AgentId,
        nativeSessionId: session.session_id,
        pinned,
      });
    },
    toggleArchived: () => {
      const showArchived = !state.showArchived;
      asyncOperations.beginNavigation(taskListNavigationTarget(showArchived), showArchived);
      dispatch({ type: "archive:set", showArchived });
      // Both lifecycle collections are live session replicas; navigation only selects one.
    },
  };

  function mutateNativeSessionArchive(
    action: "archive" | "restore",
    session: import("@openaide/app-shell-contracts").AgentListedSession,
  ) {
    const agentId = session.agent_id ?? state.newTask.selection.agentId;
    const key = nativeSessionMutationKey(agentId, session.session_id);
    if (state.nativeSessionMutations[key]?.state === "pending") return;
    if (!backendConnection?.request) {
      dispatch({
        type: "nativeSessionArchive:error",
        agentId,
        sessionId: session.session_id,
        action,
        message: "App Server connection unavailable.",
      });
      return;
    }
    dispatch({
      type: "nativeSessionArchive:start",
      agentId,
      sessionId: session.session_id,
      action,
    });
    const method = action === "archive"
      ? NATIVE_SESSION_ARCHIVE
      : NATIVE_SESSION_RESTORE;
    void backendConnection.request(method, {
      agentId: agentId as AgentId,
      nativeSessionId: session.session_id,
    }).then(() => {
      dispatch({
        type: "nativeSessionArchive:complete",
        agentId,
        sessionId: session.session_id,
      });
    }).catch((error) => {
      dispatch({
        type: "nativeSessionArchive:error",
        agentId,
        sessionId: session.session_id,
        action,
        message: error instanceof Error
          ? error.message
          : `Unable to ${action} Native Session.`,
      });
    });
  }

  function mutateNativeSessionFork(
    key: string,
    source: import("@openaide/app-server-client").NativeSessionForkSource,
  ) {
    if (pendingForkKeys.has(key) || state.nativeSessionMutations[key]?.state === "pending") return;
    if (!backendConnection?.request) {
      dispatch({
        type: "nativeSessionFork:error",
        key,
        unknown: true,
        message: "Fork may have been created. Refresh sessions to check.",
      });
      scheduleForkStateClear(key);
      return;
    }
    pendingForkKeys.add(key);
    dispatch({ type: "nativeSessionFork:start", key });
    void backendConnection.request(NATIVE_SESSION_FORK, { source }).then((result) => {
      pendingForkKeys.delete(key);
      dispatch({
        type: "nativeSessionFork:complete",
        key,
        closeWarning: result.closeWarning,
      });
      scheduleForkStateClear(key);
    }).catch((error) => {
      pendingForkKeys.delete(key);
      // Internal failures can occur after the Agent accepted the non-idempotent
      // fork but before OpenAIDE persisted or returned its identity.
      const definite = error instanceof AppServerProtocolError
        && error.protocolError.code !== "internal";
      dispatch({
        type: "nativeSessionFork:error",
        key,
        unknown: !definite,
        message: definite
          ? "Couldn't fork this session. Try again."
          : "Fork may have been created. Refresh sessions to check.",
      });
      scheduleForkStateClear(key);
    });
  }

  function scheduleForkStateClear(key: string) {
    setTimeout(() => dispatch({ type: "nativeSessionFork:clear", key }), 5_000);
  }
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
