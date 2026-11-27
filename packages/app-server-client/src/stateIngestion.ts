import type {
  AppServerEvent,
  NavigationSubscribeResult,
  StateSubscribeResult,
  SubscriptionScope,
} from "./generated/protocol.js";
import type { ReplicaSnapshot } from "./backendConnection.js";
import { updateSubscriptionSnapshot } from "./stateIngestionSnapshots.js";
import { installSubagentHistoryReply, installToolDetailReply } from "./stateIngestionEntities.js";
import { installTaskReply } from "./stateIngestionTask.js";
import { installNavigationReply } from "./stateIngestionTaskNavigation.js";
import { subscriptionScopesEqual } from "./subscriptionScope.js";

export type SubscriptionIngestionState = {
  scope: SubscriptionScope;
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
      reason: "missingChatItem" | "taskRevisionGap" | "toolDetailRevisionGap" | "subagentHistoryRevisionGap" | "missingNavigationRow" | "missingNavigationGroup";
      event: AppServerEvent;
    };

/** Installs a subscribe reply that carries a complete baseline. */
export function createSubscriptionIngestionState(
  result: Omit<StateSubscribeResult, "snapshot"> & { snapshot: ReplicaSnapshot },
): SubscriptionIngestionState {
  return { scope: result.scope, snapshot: result.snapshot };
}

/** Installs a `navigation/subscribe` reply: its frame is the view's complete baseline. */
export function installNavigationSubscribeReply(
  result: NavigationSubscribeResult,
  scope: SubscriptionScope,
): SubscriptionIngestionState | undefined {
  const navigation = installNavigationReply(result);
  if (!navigation) return undefined;
  return createSubscriptionIngestionState({ scope, snapshot: { kind: "taskNavigation", navigation } });
}

/**
 * Installs any subscribe reply. A Task, Subagent History or Tool detail reply is built on
 * `retained`, the copy the client offered as its known version (ADR-0086); `undefined` means it
 * could not be, and the caller must subscribe again without one.
 */
export function installSubscriptionReply(
  result: StateSubscribeResult,
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
  return createSubscriptionIngestionState({ ...result, snapshot: installed });
}

/**
 * Applies one event to a replica. App Server addresses each event to the subscriptions it is
 * for; the revision rules in the per-kind updates drop an event that is already in the copy and
 * ask for a baseline when one is missing.
 */
export function applySubscriptionEvent(
  state: SubscriptionIngestionState,
  event: AppServerEvent,
): SubscriptionEventApplyResult {
  if (!subscriptionScopesEqual(state.scope, event.subscription)) {
    return { kind: "ignored", state, reason: "subscriptionMismatch", event };
  }

  const update = updateSubscriptionSnapshot(state.scope, state.snapshot, event.payload);
  if (update.kind === "resyncRequired") {
    return { kind: "resyncRequired", state, reason: update.reason, event };
  }

  return {
    kind: "applied",
    state: { ...state, snapshot: update.snapshot },
    snapshotChanged: update.changed,
    event,
  };
}
