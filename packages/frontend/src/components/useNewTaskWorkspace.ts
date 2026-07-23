import { useCallback, useEffect, useRef, type Dispatch, type MutableRefObject } from "react";
import { defaultAgent } from "@openaide/app-shell-contracts";
import type { AgentOption } from "../state/composerOptions";
import type { AppAction } from "../state/appReducer";
import type { AppState } from "../state/store";
import type { WebviewBootstrap } from "../state/surfaceTypes";
import type { AsyncOperationOwner } from "../state/asyncOperationOwner";
import { retainNewTaskContext } from "../state/newTaskSelectionDefaults";
import { useComposerAttachmentResources } from "./useComposerAttachmentResources";
import { useNewTaskPreparation, type PendingNewTaskPreparation } from "./useNewTaskPreparation";
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

/** Owns New Task preparation, resources, and retained selection. */
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
  const appliedNewTaskBootstrap = useRef<WebviewBootstrap | undefined>(undefined);

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

  useEffect(() => {
    if (
      bootstrap.surface === "task"
      && !bootstrap.taskId
      && newTaskBootstrapProjectId
      && appliedNewTaskBootstrap.current !== bootstrap
      && state.newTask.selection.projectId !== newTaskBootstrapProjectId
    ) {
      // A shell Project hint seeds this route once; selector changes made after
      // the surface opens remain Frontend-owned New Task context.
      appliedNewTaskBootstrap.current = bootstrap;
      newTaskDispatch({ type: "newTask:projectId", projectId: newTaskBootstrapProjectId });
      return;
    }
    if (bootstrap.surface === "task" && !bootstrap.taskId && newTaskBootstrapProjectId) {
      appliedNewTaskBootstrap.current = bootstrap;
    }
  }, [bootstrap, newTaskBootstrapProjectId, newTaskDispatch, state.newTask.selection.projectId]);

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

  return {
    attachmentResources,
    dispatch: newTaskDispatch,
    pendingPreparationForKey: (key: string) => (
      pendingPreparation.current?.key === key ? pendingPreparation.current.promise : undefined
    ),
  };
}
