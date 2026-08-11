export type TaskStatus = "active" | "stopping" | "inactive" | "failed" | "completed" | "waiting";
export type IsolationKind = "local" | "git_worktree" | "docker";
export type ActivityStatus = "running" | "completed" | "interrupted" | "error";
export type PermissionState = "pending" | "responding" | "resolved" | "cancelled";
/** Exact ACP permission semantics retained for presentation and decisions. */
export type PermissionOptionKind =
  | "allow_once"
  | "allow_always"
  | "reject_once"
  | "reject_always"
  | "other";
export type PermissionDecision = "approved" | "denied";
export type InterruptionReason = "canceled" | "failed" | "backend_unavailable";
export type ConfigOptionsStatus = "loading" | "ready" | "empty" | "stale" | "unavailable" | "failed";
export type ConfigOptionCategory = "mode" | "model" | "thought_level" | "other";
export type AgentProbeStatus = "ready";
export type AgentAuthenticateStatus = "authenticated";
export type TaskCreateMode = "prompt_start" | "adopt_external_session";
export type DeleteMode = "archive" | "restore" | "delete";
export type RpcId = number | string;
