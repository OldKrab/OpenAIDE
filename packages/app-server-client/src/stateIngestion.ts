import type {
  AppServerEvent,
  EventCursor,
  NavigationSubscribeResult,
  StateSubscribeResult,
  SubscriptionScope,
} from "./generated/protocol.js";
import type { ReplicaSnapshot } from "./backendConnection.js";
import { updateSubscriptionSnapshot } from "./stateIngestionSnapshots.js";
import { installSubagentHistoryReply, installToolDetailReply } from "./stateIngestionEntities.js";
import { installTaskReply } from "./stateIngestionTask.js";
import { installNavigationReply } from "./stateIngestionTaskNavigation.js";
import { matchSubscriptionEvent } from "./subscriptionScope.js";
import type { SubscriptionIngestionContext } from "./subscriptionScope.js";
export type { SubscriptionIngestionContext } from "./subscriptionScope.js";

export type SubscriptionIngestionState = {
  context: SubscriptionIngestionContext;
  scope: SubscriptionScope;
  cursor: EventCursor;
  snapshot: ReplicaSnapshot;
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
      reason: "cursorGap" | "cursorDidNotAdvance" | "missingChatItem" | "streamScopeMismatch" | "taskRevisionGap" | "toolDetailRevisionGap" | "subagentHistoryRevisionGap" | "missingNavigationRow" | "missingNavigationGroup";
      event: AppServerEvent;
    };

/** Installs a subscribe reply that carries a complete baseline. */
export function createSubscriptionIngestionState(
  result: Omit<StateSubscribeResult, "snapshot"> & { snapshot: ReplicaSnapshot },
  context: SubscriptionIngestionContext,
): SubscriptionIngestionState {
  return {
    context,
    scope: result.scope,
    cursor: result.cursor,
    snapshot: result.snapshot,
  };
}

/** Installs a `navigation/subscribe` reply: its frame is the view's complete baseline. */
export function installNavigationSubscribeReply(
  result: NavigationSubscribeResult,
  scope: SubscriptionScope,
  context: SubscriptionIngestionContext,
): SubscriptionIngestionState | undefined {
  const navigation = installNavigationReply(result);
  if (!navigation) return undefined;
  return createSubscriptionIngestionState(
    { scope, cursor: result.cursor, snapshot: { kind: "taskNavigation", navigation } },
    context,
  );
}

/**
 * Installs any subscribe reply. A Task, Subagent History or Tool detail reply is built on
 * `retained`, the copy the client offered as its known version (ADR-0086); `undefined` means it
 * could not be, and the caller must subscribe again without one.
 */
export function installSubscriptionReply(
  result: StateSubscribeResult,
  context: SubscriptionIngestionContext,
  retained?: ReplicaSnapshot,
): SubscriptionIngestionState | undefined {
  const { snapshot } = result;
  let installed: ReplicaSnapshot | undefined;
  switch (snapshot.kind) {
    case "task":
      installed = installTaskReply(snapshot, retained?.kind === "task" ? retained : undefined);
      break;
    case "toolDetail":
      installed = installToolDetailReply(snapshot, retained?.kind === "toolDetail" ? retained : undefined);
      break;
    case "subagentHistory":
      installed = installSubagentHistoryReply(
        snapshot,
        retained?.kind === "subagentHistory" ? retained : undefined,
      );
      break;
    default:
      installed = snapshot;
  }
  if (!installed) return undefined;
  return createSubscriptionIngestionState({ ...result, snapshot: installed }, context);
}

export function applySubscriptionEvent(
  state: SubscriptionIngestionState,
  event: AppServerEvent,
): SubscriptionEventApplyResult {
  const match = matchSubscriptionEvent(state.scope, state.context, event);
  if (match.kind === "streamScopeMismatch") {
    return { kind: "resyncRequired", state, reason: "streamScopeMismatch", event };
  }

  // Every subscription owns its cursor. Events for another subscription are
  // unrelated transport traffic and must not advance or invalidate this replica.
  if (match.kind === "subscriptionMismatch") {
    return { kind: "ignored", state, reason: "subscriptionMismatch", event };
  }

  if (event.cursor === state.cursor) {
    return {
      kind: "ignored",
      state,
      reason: "subscriptionMismatch",
      event,
    };
  }

  if (event.previousCursor !== state.cursor) {
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
