import type { AgentId, ProjectId } from "@openaide/app-server-client";
import { projectIdForWorkspaceRoot } from "./projectIdentity";
import type { AppState } from "./store";

type PreparedTaskIdentity = {
  agentId: string;
  projectId?: string;
  workspaceRoot?: string;
};

/** Identifies the immutable Project/Agent/workspace boundary for one prepared Task. */
export function newTaskPreparationKey(state: Pick<AppState, "newTask">) {
  const context = newTaskPreparationContext(state);
  if (!context) return undefined;
  return `${context.projectId}\u0000${context.workspaceRoot ?? ""}\u0000${context.agentId}`;
}

/** Immutable Task ownership fields captured when preparing a New Task session. */
export function newTaskPreparationContext(state: Pick<AppState, "newTask">) {
  const projectId = state.newTask.selection.projectId;
  if (!projectId) return undefined;
  return {
    agentId: state.newTask.selection.agentId,
    projectId,
    workspaceRoot: taskCreateWorkspaceRoot(state),
  };
}

export function preparedTaskMatchesNewTaskContext(
  state: Pick<AppState, "newTask">,
  task: PreparedTaskIdentity,
) {
  const context = newTaskPreparationContext(state);
  return context !== undefined
    && task.projectId === context.projectId
    && task.agentId === context.agentId
    // The protocol Task summary omits workspaceRoot; derived Project identity
    // already binds that root. App-shell snapshots verify it when available.
    && (
      context.workspaceRoot === undefined
      || task.workspaceRoot === undefined
      || task.workspaceRoot === context.workspaceRoot
    );
}

export function taskCreateParams(state: Pick<AppState, "newTask">, projectId: string) {
  const workspaceRoot = taskCreateWorkspaceRoot(state);
  const configOptions = state.newTask.selection.configOptions;
  return {
    projectId: projectId as ProjectId,
    agentId: state.newTask.selection.agentId as AgentId,
    ...(workspaceRoot ? { workspaceRoot } : {}),
    ...(Object.keys(configOptions).length > 0 ? { configOptions } : {}),
  };
}

function taskCreateWorkspaceRoot(state: Pick<AppState, "newTask">) {
  const projectId = state.newTask.selection.projectId;
  const workspaceRoot = state.newTask.selection.workspaceRoot.trim();
  if (!projectId || !workspaceRoot) return undefined;
  return projectIdForWorkspaceRoot(workspaceRoot) === projectId ? workspaceRoot : undefined;
}
