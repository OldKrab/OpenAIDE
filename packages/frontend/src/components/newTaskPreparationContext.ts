import type { AgentId, ProjectId } from "@openaide/app-server-client";
import { projectIdForWorkspaceRoot } from "../state/projectIdentity";
import type { AppState } from "../state/store";

/** Identifies the immutable Project/Agent boundary for one prepared Task. */
export function newTaskPreparationKey(state: Pick<AppState, "newTask">) {
  const projectId = state.newTask.selection.projectId;
  if (!projectId) return undefined;
  const workspaceRoot = taskCreateWorkspaceRoot(state);
  return `${projectId}\u0000${workspaceRoot ?? ""}\u0000${state.newTask.selection.agentId}`;
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
