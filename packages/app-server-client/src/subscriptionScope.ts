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
    case "taskNavigation":
      return right.kind === "taskNavigation" && left.section === right.section;
    case "task":
      return right.kind === "task" && left.taskId === right.taskId;
    case "subagentCatalog":
      return right.kind === "subagentCatalog" && left.taskId === right.taskId;
    case "subagentHistory":
      return right.kind === "subagentHistory"
        && left.taskId === right.taskId
        && left.subagentId === right.subagentId;
    case "toolDetail":
      return right.kind === "toolDetail"
        && left.taskId === right.taskId
        && left.artifactId === right.artifactId;
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
  if (
    scope.kind === "task"
    || scope.kind === "subagentCatalog"
    || scope.kind === "subagentHistory"
    || scope.kind === "toolDetail"
  ) {
    return eventScope.kind === "task" && eventScope.taskId === scope.taskId
      ? { kind: "match" }
      : { kind: "subscriptionMismatch" };
  }

  if (eventScope.kind !== "client") return { kind: "subscriptionMismatch" };

  if (context.clientInstanceId === undefined) return { kind: "streamScopeMismatch" };

  if (eventScope.clientInstanceId === context.clientInstanceId) {
    return { kind: "match" };
  }

  return { kind: "streamScopeMismatch" };
}

function payloadMatchesSubscriptionScope(scope: SubscriptionScope, payload: AppServerEventPayload): boolean {
  switch (scope.kind) {
    case "taskNavigation":
      return payload.kind === "navigationFrame" && payload.frame.section === scope.section;
    case "task":
      return (
        payload.kind === "taskChanged" ||
        payload.kind === "taskHistorySyncUpdated" ||
        payload.kind === "taskRequestsUpdated"
      );
    // A deleted Task ends each entity subscription with `taskContentRemoved`.
    case "subagentCatalog":
      return (payload.kind === "subagentCatalogUpdated" && payload.catalog.taskId === scope.taskId)
        || taskContentRemoved(payload, scope.taskId);
    case "subagentHistory":
      return (
        payload.kind === "subagentHistoryChanged"
        && payload.taskId === scope.taskId
        && payload.subagentId === scope.subagentId
      ) || taskContentRemoved(payload, scope.taskId);
    case "toolDetail":
      return (
        payload.kind === "toolDetailChanged"
        && payload.taskId === scope.taskId
        && payload.artifactId === scope.artifactId
      ) || taskContentRemoved(payload, scope.taskId);
  }
}

function taskContentRemoved(payload: AppServerEventPayload, taskId: string): boolean {
  return payload.kind === "taskContentRemoved" && payload.taskId === taskId;
}
