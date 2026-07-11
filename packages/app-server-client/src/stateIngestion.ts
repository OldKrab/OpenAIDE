import type {
  AppServerEvent,
  EventCursor,
  StateSubscribeResult,
  SubscriptionScope,
  SubscriptionSnapshot,
} from "./generated/protocol.js";
import { updateSubscriptionSnapshot } from "./stateIngestionSnapshots.js";
import { matchSubscriptionEvent } from "./subscriptionScope.js";
import type { SubscriptionIngestionContext } from "./subscriptionScope.js";
export type { SubscriptionIngestionContext } from "./subscriptionScope.js";

export type SubscriptionIngestionState = {
  context: SubscriptionIngestionContext;
  scope: SubscriptionScope;
  cursor: EventCursor;
  snapshot: SubscriptionSnapshot;
};

export type SubscriptionEventApplyResult =
  | {
      kind: "applied";
      state: SubscriptionIngestionState;
      snapshotChanged: boolean;
      event: AppServerEvent;
    }
  | {
      kind: "ignored";
      state: SubscriptionIngestionState;
      reason: "subscriptionMismatch";
      event: AppServerEvent;
    }
  | {
      kind: "resyncRequired";
      state: SubscriptionIngestionState;
      reason: "cursorGap" | "cursorDidNotAdvance" | "missingChatItem" | "streamScopeMismatch";
      event: AppServerEvent;
    };

export function createSubscriptionIngestionState(
  result: StateSubscribeResult,
  context: SubscriptionIngestionContext,
): SubscriptionIngestionState {
  return {
    context,
    scope: result.scope,
    cursor: result.cursor,
    snapshot: result.snapshot,
  };
}

export function applySubscriptionEvent(
  state: SubscriptionIngestionState,
  event: AppServerEvent,
): SubscriptionEventApplyResult {
  const match = matchSubscriptionEvent(state.scope, state.context, event);
  if (match.kind === "streamScopeMismatch") {
    return { kind: "resyncRequired", state, reason: "streamScopeMismatch", event };
  }

  if (event.cursor === state.cursor) {
    return {
      kind: "ignored",
      state,
      reason: "subscriptionMismatch",
      event,
    };
  }

  if (match.kind === "subscriptionMismatch") {
    return {
      kind: "ignored",
      state: {
        ...state,
        cursor: event.cursor,
      },
      reason: "subscriptionMismatch",
      event,
    };
  }

  if (event.previousCursor !== state.cursor && !canApplyWithCursorGap(event)) {
    return { kind: "resyncRequired", state, reason: "cursorGap", event };
  }

  const update = updateSubscriptionSnapshot(state.scope, state.snapshot, event.payload);
  if (update.kind === "resyncRequired") {
    return { kind: "resyncRequired", state, reason: update.reason, event };
  }

  return {
    kind: "applied",
    state: {
      ...state,
      cursor: event.cursor,
      snapshot: update.snapshot,
    },
    snapshotChanged: update.changed,
    event,
  };
}

function canApplyWithCursorGap(event: AppServerEvent) {
  if (event.scope.kind !== "stateRoot") return false;
  return (
    event.payload.kind === "projectCollectionUpdated" ||
    event.payload.kind === "taskNavigationUpdated" ||
    event.payload.kind === "snapshotReplaced"
  );
}
