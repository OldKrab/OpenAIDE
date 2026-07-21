import type {
  AppServerEvent,
  AppServerEventPayload,
  ClientInstanceId,
  EventScope,
  StateRootId,
  SubscriptionScope,
} from "./generated/protocol.js";

export type SubscriptionIngestionContext = {
  stateRootId: StateRootId;
  clientInstanceId?: ClientInstanceId;
};

export type SubscriptionEventMatch =
  | { kind: "match" }
  | { kind: "subscriptionMismatch" }
  | { kind: "streamScopeMismatch" };

export function subscriptionScopesEqual(left: SubscriptionScope, right: SubscriptionScope): boolean {
  switch (left.kind) {
    case "projects":
    case "agents":
      return right.kind === left.kind;
    case "settings":
      return right.kind === "settings" && normalizeOptional(left.section) === normalizeOptional(right.section);
    case "taskNavigation":
      return right.kind === "taskNavigation" && normalizeOptional(left.projectId) === normalizeOptional(right.projectId);
    case "task":
      return right.kind === "task" && left.taskId === right.taskId;
    case "toolDetail":
      return right.kind === "toolDetail"
        && left.taskId === right.taskId
        && left.artifactId === right.artifactId;
    case "worktreeRepository":
      return right.kind === "worktreeRepository" && left.repositoryId === right.repositoryId;
  }
}

export function matchSubscriptionEvent(
  scope: SubscriptionScope,
  context: SubscriptionIngestionContext,
  event: AppServerEvent,
): SubscriptionEventMatch {
  if (!subscriptionScopesEqual(scope, event.subscription)) return { kind: "subscriptionMismatch" };
  if (eventScopeStateRootId(event.scope) !== context.stateRootId) return { kind: "streamScopeMismatch" };

  const scopeMatch = eventScopeMatchesSubscriptionScope(scope, event.scope, context);
  if (scopeMatch.kind !== "match") return scopeMatch;

  return payloadMatchesSubscriptionScope(scope, event.payload) ? { kind: "match" } : { kind: "subscriptionMismatch" };
}

function eventScopeStateRootId(scope: EventScope): StateRootId {
  return scope.stateRootId;
}

function eventScopeMatchesSubscriptionScope(
  scope: SubscriptionScope,
  eventScope: EventScope,
  context: SubscriptionIngestionContext,
): SubscriptionEventMatch {
  if (scope.kind === "task" || scope.kind === "toolDetail") {
    return eventScope.kind === "task" && eventScope.taskId === scope.taskId
      ? { kind: "match" }
      : { kind: "subscriptionMismatch" };
  }

  if (eventScope.kind === "stateRoot") return { kind: "match" };
  if (eventScope.kind !== "client") return { kind: "subscriptionMismatch" };

  if (context.clientInstanceId === undefined) return { kind: "streamScopeMismatch" };

  if (eventScope.clientInstanceId === context.clientInstanceId) {
    return { kind: "match" };
  }

  return { kind: "streamScopeMismatch" };
}

function payloadMatchesSubscriptionScope(scope: SubscriptionScope, payload: AppServerEventPayload): boolean {
  switch (scope.kind) {
    case "projects":
      return payload.kind === "snapshotReplaced" || payload.kind === "projectCollectionUpdated";
    case "agents":
      return payload.kind === "snapshotReplaced" || payload.kind === "agentCollectionUpdated";
    case "settings":
      return payload.kind === "snapshotReplaced";
    case "taskNavigation":
      return (
        payload.kind === "snapshotReplaced" ||
        payload.kind === "taskNavigationChanged"
      );
    case "task":
      return (
        payload.kind === "snapshotReplaced" ||
        payload.kind === "taskChanged" ||
        payload.kind === "taskHistorySyncUpdated" ||
        payload.kind === "taskRequestsUpdated" ||
        payload.kind === "requestUpdated"
      );
    case "toolDetail":
      return (payload.kind === "toolDetailUpdated" || payload.kind === "toolDetailChanged")
        && payload.taskId === scope.taskId
        && payload.artifactId === scope.artifactId;
    case "worktreeRepository":
      return payload.kind === "worktreeRepositoryUpdated"
        && payload.repositoryId === scope.repositoryId;
  }
}

function normalizeOptional(value: string | null | undefined): string | undefined {
  return value ?? undefined;
}
