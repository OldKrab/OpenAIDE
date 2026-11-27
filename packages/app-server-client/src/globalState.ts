import type { GlobalState, GlobalStateChanges } from "./generated/protocol.js";

/**
 * Applies one `sync` notification to a copy of the global state (ADR-0086). An area present in
 * `changes` replaces the held one as a whole; worktree repositories are keyed by `repositoryId`.
 * All areas move in one step, so a reader never sees a half-applied change.
 */
export function applyGlobalStateChanges(state: GlobalState, changes: GlobalStateChanges): GlobalState {
  const removed = new Set(changes.removedWorktreeRepositories ?? []);
  const upserts = new Map((changes.worktrees ?? []).map((repository) => [repository.repositoryId, repository]));
  const worktrees = state.worktrees
    .filter((repository) => !removed.has(repository.repositoryId))
    .map((repository) => upserts.get(repository.repositoryId) ?? repository);
  const known = new Set(worktrees.map((repository) => repository.repositoryId));
  for (const [repositoryId, repository] of upserts) {
    if (!known.has(repositoryId)) worktrees.push(repository);
  }
  return {
    projects: changes.projects ?? state.projects,
    worktrees,
    agents: changes.agents ?? state.agents,
    settings: changes.settings ?? state.settings,
    newTaskDefaults: changes.newTaskDefaults ?? state.newTaskDefaults,
    devices: changes.devices ?? state.devices,
    terminals: changes.terminals ?? state.terminals,
  };
}
