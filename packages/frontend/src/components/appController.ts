import { useEffect, useMemo, useReducer, useRef, useState } from "react";
import type { Dispatch } from "react";
import type { AppPreferencesRecord, TaskSummary } from "@openaide/app-shell-contracts";
import {
  AGENT_CONFIG_OPTIONS,
  TASK_SEND,
  type AgentId,
  type ProjectId,
  type TaskId,
} from "@openaide/app-server-client";
import { defaultAgent } from "@openaide/app-shell-contracts";
import {
  getBackendConnection,
  getBootstrap,
  postHostMessage,
} from "../services/hostBridge";
import {
  clearPendingTaskSendRecovery,
  readPendingTaskSendRecovery,
} from "../services/pendingTaskSendRecovery";
import { mapProtocolConfigOptions } from "../state/appServerConfigOptions";
import { appReducer, type AppAction, type SnapshotIntent } from "../state/appReducer";
import { mapProtocolTaskSnapshot } from "../state/appServerProtocolMapping";
import type { AgentOption } from "../state/composerOptions";
import { sendWebviewTelemetry } from "../state/hostMessageRouter";
import {
  agentProjectRequestKey,
  configOptionsRequestKey,
  shouldLoadNativeSessions,
  shouldLoadNewTaskConfigOptions,
} from "../state/surfaceRouting";
import type { WebviewBootstrap } from "../state/surfaceTypes";
import { createInitialState, type AppState } from "../state/store";
import { createAppCallbacks, type AppControllerCallbacks } from "./appControllerCallbacks";
import type { NewTaskStartAttempt } from "./appControllerCallbackTypes";
import {
  useAppControllerBackendLifecycle,
  type AppControllerBackendConnection,
} from "./appControllerBackendLifecycle";
import { appControllerDerivedStateDeps, deriveAppControllerState } from "./appControllerDerivedState";
import { createRequestControllerNativeSessions } from "./appControllerNativeSessions";
import { useAppControllerRefs } from "./appControllerRefs";
import { useSettingsRouteRefresh } from "./appControllerRouting";
import { useActiveTaskPolling } from "./appControllerTaskPolling";
import { useNewTaskPreparation, type PendingNewTaskPreparation } from "./useNewTaskPreparation";
import {
  newTaskConfigOptionsContextForRequests,
  newTaskProjectIdForRequests,
} from "./newTaskRequestContext";

const NEW_TASK_CONFIG_OPTIONS_TIMEOUT_MS = 10_000;
export type AppController = {
  activeTask?: TaskSummary;
  activeNavigationTaskId?: string;
  agents?: AgentOption[];
  backendReady: boolean;
  bootstrap: WebviewBootstrap;
  callbacks: AppControllerCallbacks;
  createSnapshotRequestId: (taskId?: string, intent?: SnapshotIntent) => number;
  dispatch: Dispatch<AppAction>;
  preferences: AppPreferencesRecord;
  state: AppState;
  visibleTasks: AppState["tasks"];
};

export type AppControllerOptions = {
  backendConnection?: AppControllerBackendConnection;
};
export function useAppController({ backendConnection }: AppControllerOptions = {}) {
  const backendConnectionRef = useMemo(() => backendConnection ?? getBackendConnection(), [backendConnection]);
  const initialBootstrap = useMemo(() => getBootstrap(), []);
  const [state, dispatch] = useReducer(appReducer, undefined, createInitialState);
  const [preferences, setPreferences] = useState<AppPreferencesRecord>(initialBootstrap.preferences ?? { composer_submit_shortcut: "mod_enter" });
  const [agents, setAgents] = useState<AgentOption[] | undefined>(undefined);
  const currentAgentId = useRef(state.newTask.selection.agentId);
  currentAgentId.current = state.newTask.selection.agentId;
  const pendingPreparedNewTask = useRef<PendingNewTaskPreparation | undefined>(undefined);
  const newTaskStartAttempt = useRef<NewTaskStartAttempt | undefined>(undefined);
  const recoveringTaskSend = useRef<string | undefined>(undefined);
  const controllerRefs = useAppControllerRefs();
  const {
    latestNativeSessionSelection,
    latestNavigationSessionKey,
    latestOptionsRequestKey,
    latestSessionListRequestId,
    nextSessionListRequestId,
  } = controllerRefs;
  const {
    acceptSnapshotRequest,
    backendInitialized,
    backendReady,
    beginNavigationChange,
    bootstrap,
    createSnapshotRequestId,
    currentNavigationGeneration,
  } = useAppControllerBackendLifecycle({
    backendConnection: backendConnectionRef,
    currentAgentId,
    dispatch,
    initialBootstrap,
    refs: controllerRefs,
    setAgents,
    setPreferences,
    state,
  });
  const newTaskBootstrapProjectId = bootstrap.surface === "task" && !bootstrap.taskId
    ? bootstrap.projectId
    : undefined;
  useNewTaskPreparation({
    backendConnection: backendConnectionRef,
    backendReady,
    bootstrap,
    currentNavigationGeneration,
    dispatch,
    latestOptionsRequestKey,
    pendingPreparation: pendingPreparedNewTask,
    startAttempt: newTaskStartAttempt,
    state,
  });
  const pendingPreparedNewTaskForKey = (key: string) =>
    pendingPreparedNewTask.current?.key === key ? pendingPreparedNewTask.current.promise : undefined;
  const requestNativeSessions = createRequestControllerNativeSessions({
    backendConnection: backendConnectionRef,
    dispatch,
    getAgentId: () => state.newTask.selection.agentId,
    getProjectId: () => state.newTask.selection.projectId,
    latestSessionListRequestId,
    nextSessionListRequestId,
  });
  useEffect(() => {
    if (
      bootstrap.surface === "task" &&
      !bootstrap.taskId &&
      newTaskBootstrapProjectId &&
      state.newTask.selection.projectId !== newTaskBootstrapProjectId
    ) {
      dispatch({ type: "newTask:projectId", projectId: newTaskBootstrapProjectId });
    }
  }, [bootstrap.surface, bootstrap.taskId, newTaskBootstrapProjectId, state.newTask.selection.projectId]);

  useEffect(() => {
    if (bootstrap.surface !== "task" || bootstrap.taskId || !agents?.length) return;
    const selected = agents.find((agent) => agent.id === state.newTask.selection.agentId);
    if (selected && selected.enabled !== false) return;
    const fallback = agents.find((agent) => agent.enabled !== false) ?? defaultAgent;
    latestOptionsRequestKey.current = undefined;
    dispatch({ type: "newTask:agent", agentId: fallback.id, agentLabel: fallback.label });
  }, [agents, bootstrap.surface, bootstrap.taskId, state.newTask.selection.agentId]);

  useEffect(() => {
    if (
      bootstrap.surface !== "task" ||
      !bootstrap.taskId ||
      !backendReady ||
      !backendConnectionRef?.request ||
      !state.snapshot ||
      state.snapshot.task.task_id !== bootstrap.taskId ||
      state.snapshot.task.has_messages
    ) {
      return;
    }
    const recovery = readPendingTaskSendRecovery(bootstrap.taskId);
    if (!recovery || recoveringTaskSend.current === recovery.taskId) return;
    recoveringTaskSend.current = recovery.taskId;
    dispatch({ type: "taskInput:prompt", taskId: recovery.taskId, prompt: recovery.renderState.prompt });
    for (const attachment of recovery.renderState.context) {
      dispatch({ type: "taskInput:attachment:addAppServer", taskId: recovery.taskId, attachment });
    }
    dispatch({ type: "taskInput:submit", taskId: recovery.taskId });
    void backendConnectionRef.request(TASK_SEND, {
      taskId: recovery.taskId as TaskId,
      taskRevision: recovery.taskRevision,
      idempotencyKey: recovery.idempotencyKey,
      message: recovery.message,
    }).then((result) => {
      clearPendingTaskSendRecovery(recovery.taskId);
      dispatch({
        type: "snapshot",
        snapshot: mapProtocolTaskSnapshot(result.task).snapshot,
        intent: "refresh",
      });
    }).catch((error) => {
      clearPendingTaskSendRecovery(recovery.taskId);
      dispatch({
        type: "taskInput:error",
        taskId: recovery.taskId,
        message: error instanceof Error ? error.message : "Unable to recover submitted message.",
      });
    }).finally(() => {
      if (recoveringTaskSend.current === recovery.taskId) {
        recoveringTaskSend.current = undefined;
      }
    });
  }, [
    backendConnectionRef,
    backendReady,
    bootstrap.surface,
    bootstrap.taskId,
    state.snapshot?.task.task_id,
    state.snapshot?.task.has_messages,
  ]);

  useEffect(() => {
    const snapshotTaskId = state.snapshot?.task.task_id;
    const snapshotHasPendingInput = snapshotTaskId
      ? state.taskInputs[snapshotTaskId]?.pending !== undefined
      : false;
    const snapshotHasPendingRecovery = snapshotTaskId
      ? readPendingTaskSendRecovery(snapshotTaskId) !== undefined
      : false;
    if (
      bootstrap.surface !== "task" ||
      !bootstrap.taskId ||
      !state.snapshot ||
      state.snapshot.task.has_messages ||
      state.snapshot.task.status !== "inactive" ||
      snapshotHasPendingInput ||
      snapshotHasPendingRecovery
    ) {
      return;
    }
    postHostMessage({
      type: "surface.openNewTask",
      payload: state.snapshot.task.project_id
        ? { project_id: state.snapshot.task.project_id }
        : undefined,
    });
  }, [
    bootstrap.surface,
    bootstrap.taskId,
    state.snapshot?.task.task_id,
    state.snapshot?.task.has_messages,
    state.snapshot?.task.status,
    state.snapshot?.task.project_id,
    state.snapshot ? state.taskInputs[state.snapshot.task.task_id]?.pending : undefined,
  ]);
  useSettingsRouteRefresh({
    backendConnectionRef,
    backendInitialized,
    bootstrap,
    currentAgentId,
    dispatch,
    setAgents,
    state,
  });
  useActiveTaskPolling({
    backendConnectionRef,
    backendInitialized,
    createSnapshotRequestId,
    dispatch,
    postHostMessage,
    state,
  });
  useEffect(() => {
    const projectContext = newTaskConfigOptionsContextForRequests(state, newTaskBootstrapProjectId);
    const projectId = projectContext?.projectId;
    if (
      !backendReady ||
      !shouldLoadNewTaskConfigOptions(
        bootstrap,
        state.snapshot !== undefined,
        projectId,
      )
    ) {
      return;
    }
    if (!projectId) return;
    const key = configOptionsRequestKey(
      state.newTask.selection.agentId,
      projectId,
      projectContext.workspaceRoot,
    );
    if (latestOptionsRequestKey.current === key) return;
    latestOptionsRequestKey.current = key;
    dispatch({ type: "newTask:configOptions:start" });
    if (backendConnectionRef?.request) {
      void withTimeout(
        backendConnectionRef.request(AGENT_CONFIG_OPTIONS, {
          agentId: state.newTask.selection.agentId as AgentId,
          projectId: projectId as ProjectId,
          ...(projectContext.workspaceRoot ? { workspaceRoot: projectContext.workspaceRoot } : {}),
        }),
        NEW_TASK_CONFIG_OPTIONS_TIMEOUT_MS,
        "Agent options request timed out.",
      ).then((result) => {
        if (latestOptionsRequestKey.current !== key) return;
        dispatch({ type: "newTask:configOptions:result", catalog: mapProtocolConfigOptions(result) });
      }).catch(() => {
        if (latestOptionsRequestKey.current !== key) return;
        dispatch({ type: "newTask:configOptions:error", message: "Unable to load Agent options." });
      });
      return;
    }
    dispatch({ type: "newTask:configOptions:error", message: "App Server connection unavailable." });
  }, [
    bootstrap.surface,
    bootstrap.taskId,
    backendReady,
    state.snapshot,
    state.newTask.selection.agentId,
    state.newTask.selection.projectId,
    state.newTask.selection.workspaceRoot,
    state.projects,
    state.tasks,
    newTaskBootstrapProjectId,
  ]);

  useEffect(() => {
    const projectId = newTaskProjectIdForRequests(state, newTaskBootstrapProjectId);
    if (
      !backendReady ||
      !shouldLoadNativeSessions(
        bootstrap,
        projectId,
      )
    ) {
      return;
    }
    if (!projectId) return;
    const key = agentProjectRequestKey(state.newTask.selection.agentId, projectId);
    if (latestNavigationSessionKey.current === key) return;
    latestNavigationSessionKey.current = key;
    latestNativeSessionSelection.current = {
      agentId: state.newTask.selection.agentId,
      workspaceRoot: state.newTask.selection.workspaceRoot,
    };
    requestNativeSessions();
  }, [
    backendReady,
    bootstrap.surface,
    state.newTask.selection.agentId,
    state.newTask.selection.projectId,
    state.newTask.selection.workspaceRoot,
    state.projects,
    state.tasks,
    newTaskBootstrapProjectId,
  ]);

  const derivedStateDeps = appControllerDerivedStateDeps(state);
  const { activeNavigationTaskId, activeTask, hasActiveTask, visibleTasks } = useMemo(
    () => deriveAppControllerState(state),
    derivedStateDeps,
  );

  useEffect(() => {
    if (!state.snapshot) return;
    sendWebviewTelemetry(postHostMessage, "task_rendered", {
      surface: bootstrap.surface,
      task_id: state.snapshot.task.task_id,
      task_status: state.snapshot.task.status,
      chat_items: state.snapshot.chat.items.length,
      has_active_task: hasActiveTask,
    });
  }, [hasActiveTask, bootstrap.surface, state.snapshot?.task.task_id, state.snapshot?.task.status, state.snapshot?.chat.items.length]);

  const callbacks = createAppCallbacks({
    acceptTaskOpen: acceptSnapshotRequest,
    backendConnection: backendConnectionRef,
    beginNavigationChange,
    createSnapshotRequestId,
    currentNavigationGeneration,
    dispatch,
    latestOptionsRequestKey,
    newTaskStartAttempt,
    pendingPreparedNewTask: pendingPreparedNewTaskForKey,
    requestNativeSessions,
    setAgents,
    setPreferences,
    state,
  });

  return {
    activeNavigationTaskId,
    activeTask,
    agents,
    backendReady,
    bootstrap,
    callbacks,
    createSnapshotRequestId,
    dispatch,
    preferences,
    state,
    visibleTasks,
  };
}

function withTimeout<T>(promise: Promise<T>, timeoutMs: number, message: string): Promise<T> {
  let timeoutId: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timeoutId = setTimeout(() => reject(new Error(message)), timeoutMs);
  });
  return Promise.race([promise, timeout]).finally(() => {
    if (timeoutId !== undefined) clearTimeout(timeoutId);
  });
}
