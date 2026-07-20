import { useCallback, useEffect, useRef, type Dispatch, type MutableRefObject } from "react";
import { defaultAgent } from "@openaide/app-shell-contracts";
import type { AgentOption } from "../state/composerOptions";
import type { AppAction } from "../state/appReducer";
import type { AppState } from "../state/store";
import type { WebviewBootstrap } from "../state/surfaceTypes";
import type { AsyncOperationOwner } from "../state/asyncOperationOwner";
import { retainNewTaskContext } from "../state/newTaskSelectionDefaults";
import { sendWebviewTelemetry } from "../state/hostMessageRouter";
import { agentProjectRequestKey, shouldLoadNativeSessions } from "../state/surfaceRouting";
import { postHostMessage } from "../services/hostBridge";
import { useComposerAttachmentResources } from "./useComposerAttachmentResources";
import { useNewTaskPreparation, type PendingNewTaskPreparation } from "./useNewTaskPreparation";
import { createRequestControllerNativeSessions } from "./appControllerNativeSessions";
import { newTaskProjectIdForRequests } from "./newTaskRequestContext";
import type { AppControllerBackendConnection } from "./appControllerBackendLifecycle";
import type { NewTaskStartAttempt } from "./appControllerCallbackTypes";
import type { NewTaskController } from "./newTaskController";

type NewTaskWorkspaceOptions = {
  agents?: AgentOption[];
  asyncOperations: AsyncOperationOwner;
  backendConnection?: AppControllerBackendConnection;
  backendReady: boolean;
  bootstrap: WebviewBootstrap;
  clientInstanceId: string;
  dispatch: Dispatch<AppAction>;
  newTaskController: NewTaskController;
  newTaskSnapshot?: import("@openaide/app-shell-contracts").TaskSnapshot;
  pendingPreparation: MutableRefObject<PendingNewTaskPreparation | undefined>;
  replicaEpoch: number;
  startAttempt: MutableRefObject<NewTaskStartAttempt | undefined>;
  state: AppState;
};

/** Owns New Task preparation, resources, retained selection, and Native Session discovery. */
export function useNewTaskWorkspace({
  agents,
  asyncOperations,
  backendConnection,
  backendReady,
  bootstrap,
  clientInstanceId,
  dispatch,
  newTaskController,
  newTaskSnapshot,
  pendingPreparation,
  replicaEpoch,
  startAttempt,
  state,
}: NewTaskWorkspaceOptions) {
  const latestNavigationSessionKey = useRef<string | undefined>(undefined);
  const newTaskDispatch = useCallback((action: AppAction) => {
    switch (action.type) {
      case "newTask:agent":
      case "newTask:project":
      case "newTask:projectId":
      case "newTask:workspace":
      case "newTask:worktree":
        dispatch({
          ...action,
          newTaskId: action.newTaskId ?? newTaskSnapshot?.task.task_id,
        });
        return;
      default:
        dispatch(action);
    }
  }, [dispatch, newTaskSnapshot?.task.task_id]);
  const attachmentResources = useComposerAttachmentResources({
    backendConnection,
    clientInstanceId,
    dispatch: newTaskDispatch,
    newTaskId: newTaskSnapshot?.task.task_id,
    state,
    taskSurfaceMounted: bootstrap.surface === "task",
  });
  const newTaskBootstrapProjectId = bootstrap.surface === "task" && !bootstrap.taskId
    ? bootstrap.projectId
    : undefined;

  useEffect(() => {
    if (!state.appServerStateRootId) return;
    retainNewTaskContext(state.appServerStateRootId, clientInstanceId, {
      projectId: state.newTask.selection.projectId,
      agentId: state.newTask.selection.agentId || undefined,
    });
  }, [
    clientInstanceId,
    state.appServerStateRootId,
    state.newTask.selection.agentId,
    state.newTask.selection.projectId,
  ]);

  useNewTaskPreparation({
    attachmentResources,
    asyncOperations,
    backendConnection,
    backendReady,
    bootstrap,
    dispatch: newTaskDispatch,
    pendingPreparation,
    newTaskController,
    replicaEpoch,
    startAttempt,
    state,
  });

  const requestNativeSessions = createRequestControllerNativeSessions({
    backendConnection,
    dispatch,
    getAgentId: () => state.newTask.selection.agentId,
    getExistingSessionIds: () => state.newTask.nativeSessions.items.map((session) => session.session_id),
    getProjectId: () => state.newTask.selection.projectId,
    asyncOperations,
    onFailure: (failure) => sendWebviewTelemetry(postHostMessage, "native_sessions_load_failed", {
      surface: bootstrap.surface,
      request: failure.request,
      session_list_request_id: failure.requestId,
      agent_id: failure.agentId,
      project_id: failure.projectId,
      error_name: failure.errorName,
      error_code: failure.errorCode,
      error_message: failure.errorMessage,
    }),
  });

  useEffect(() => {
    if (
      bootstrap.surface === "task"
      && !bootstrap.taskId
      && newTaskBootstrapProjectId
      && state.newTask.selection.projectId !== newTaskBootstrapProjectId
    ) {
      newTaskDispatch({ type: "newTask:projectId", projectId: newTaskBootstrapProjectId });
    }
  }, [bootstrap.surface, bootstrap.taskId, newTaskBootstrapProjectId, state.newTask.selection.projectId]);

  useEffect(() => {
    const selectedWorktreeId = state.newTask.selection.worktreeId;
    if (!selectedWorktreeId) return;
    const project = state.projects.find((candidate) => candidate.projectId === state.newTask.selection.projectId);
    const repository = project?.worktreeRepositoryId
      ? state.worktreeRepositories[project.worktreeRepositoryId]
      : undefined;
    const selected = repository?.worktrees.find((worktree) => worktree.worktreeId === selectedWorktreeId);
    if (!selected?.forgotten) return;
    const projectRoot = repository?.worktrees.find((worktree) => (
      !worktree.forgotten
      && worktree.worktreeId === project?.projectWorktreeId
      && worktree.availability === "available"
    ));
    if (!projectRoot) return;
    // Repository updates reach every client; move any retained New Task draft off the removed path.
    newTaskDispatch({
      type: "newTask:worktree",
      worktreeId: undefined,
      label: "Project root",
      path: projectRoot.path,
    });
  }, [
    newTaskDispatch,
    state.newTask.selection.projectId,
    state.newTask.selection.worktreeId,
    state.projects,
    state.worktreeRepositories,
  ]);

  useEffect(() => {
    if (bootstrap.surface !== "task" || bootstrap.taskId || !agents?.length) return;
    const selected = agents.find((agent) => agent.id === state.newTask.selection.agentId);
    if (selected && selected.enabled !== false) return;
    const fallback = agents.find((agent) => agent.enabled !== false) ?? defaultAgent;
    newTaskDispatch({ type: "newTask:agent", agentId: fallback.id, agentLabel: fallback.label });
  }, [agents, bootstrap.surface, bootstrap.taskId, state.newTask.selection.agentId]);

  useEffect(() => {
    const projectId = newTaskProjectIdForRequests(state, newTaskBootstrapProjectId);
    const preparingNewTask = bootstrap.surface === "task" && !bootstrap.taskId && !newTaskSnapshot;
    // Native Session discovery only enriches Task Navigation. Let the New Task
    // acquire and subscribe first so slow Agent history cannot hide ready controls.
    if (
      !backendReady
      || preparingNewTask
      || !shouldLoadNativeSessions(bootstrap, projectId)
      || !projectId
    ) return;
    const key = `${replicaEpoch}:${agentProjectRequestKey(state.newTask.selection.agentId, projectId)}`;
    if (latestNavigationSessionKey.current === key) return;
    latestNavigationSessionKey.current = key;
    // Startup discovery is optional navigation enrichment. Keep it to one page
    // so a slow Agent cursor cannot hold uploads, heartbeats, or Task controls;
    // the returned cursor remains available through explicit Load More.
    requestNativeSessions();
  }, [
    backendReady,
    bootstrap.surface,
    bootstrap.taskId,
    newTaskSnapshot?.task.task_id,
    replicaEpoch,
    state.newTask.selection.agentId,
    state.newTask.selection.projectId,
    state.newTask.selection.workspaceRoot,
    state.projects,
    state.tasks,
    newTaskBootstrapProjectId,
  ]);

  return {
    attachmentResources,
    dispatch: newTaskDispatch,
    pendingPreparationForKey: (key: string) => (
      pendingPreparation.current?.key === key ? pendingPreparation.current.promise : undefined
    ),
    requestNativeSessions,
  };
}
