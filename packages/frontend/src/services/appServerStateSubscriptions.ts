import {
  applySubscriptionEvent,
  createSubscriptionIngestionState,
  STATE_SUBSCRIBE,
  STATE_UNSUBSCRIBE,
  type AgentSummary,
  type AppServerEvent,
  type BackendConnection,
  type ClientSnapshot,
  type ProjectSummary,
  type SubscriptionIngestionContext,
  type SubscriptionIngestionState,
  type SubscriptionScope,
  type SubscriptionSnapshot,
} from "@openaide/app-server-client";
import type { Dispatch } from "react";
import type { AppAction } from "../state/appReducer";
import { applyProtocolAgents } from "../state/appServerAgents";
import {
  mapProtocolTaskNavigation,
  mapProtocolTaskSnapshot,
} from "../state/appServerProtocolMapping";
import type { AgentOption } from "../state/composerOptions";

type StateSubscriptionConnection = Pick<BackendConnection, "events" | "request">
  & Partial<Pick<BackendConnection, "stateResets">>;
const SUBSCRIPTION_RETRY_MS = 500;
const MAX_SUBSCRIPTION_RETRY_MS = 5_000;
const MAX_PENDING_EVENTS = 1_000;
type SubscriptionLeaseState = { cleanup?: Promise<void>; count: number };
const subscriptionLeases = new WeakMap<
  BackendConnection["request"],
  Map<string, SubscriptionLeaseState>
>();

export type StateSubscriptionMappingContext = SubscriptionIngestionContext & {
  agents?: AgentSummary[];
  projects?: ProjectSummary[];
};

export function mappingContextFromClientSnapshot(snapshot: ClientSnapshot): StateSubscriptionMappingContext {
  return {
    stateRootId: snapshot.stateRoot.stateRootId,
    clientInstanceId: snapshot.client.clientInstanceId,
    agents: snapshot.agents?.agents,
    projects: snapshot.projects?.projects,
  };
}

export function startAppServerStateSubscription({
  backendConnection,
  context,
  currentAgentId,
  dispatch,
  onBaselineError,
  onBaselineLost,
  onBaselineReady,
  setAgents,
  scope,
}: {
  backendConnection: StateSubscriptionConnection;
  context: StateSubscriptionMappingContext;
  currentAgentId?: () => string;
  dispatch: Dispatch<AppAction>;
  /** Signals that events must stop mutating product state until a fresh baseline is installed. */
  onBaselineLost?: () => void;
  /** Reports a failed baseline read that will be retried with bounded backoff. */
  onBaselineError?: (error: unknown) => void;
  /** Signals that the current scope has a cursor baseline and no queued resync gap. */
  onBaselineReady?: () => void;
  setAgents?: (agents: AgentOption[]) => void;
  scope: SubscriptionScope;
}) {
  const scopeLease = acquireSubscriptionLease(backendConnection.request, scope);
  let disposed = false;
  let state: SubscriptionIngestionState | undefined;
  let pendingEvents: AppServerEvent[] = [];
  let unsubscribe = backendConnection.events(handleEvent);
  const unsubscribeStateResets = backendConnection.stateResets?.(handleStateReset);
  let subscribeInFlight = false;
  let subscribeAgain = false;
  let refreshing = true;
  let retryTimer: ReturnType<typeof setTimeout> | undefined;
  let retryDelay = SUBSCRIPTION_RETRY_MS;

  void subscribe();

  return () => {
    disposed = true;
    if (retryTimer !== undefined) clearTimeout(retryTimer);
    unsubscribe();
    unsubscribeStateResets?.();
    scopeLease.release(unsubscribeScope);
  };

  async function subscribe() {
    if (subscribeInFlight) return;
    if (retryTimer !== undefined) clearTimeout(retryTimer);
    retryTimer = undefined;
    if (!refreshing) onBaselineLost?.();
    refreshing = true;
    subscribeInFlight = true;
    try {
      const pendingCleanup = scopeLease.waitForCleanup();
      if (pendingCleanup) await pendingCleanup;
      if (disposed) return;
      const result = await backendConnection.request(STATE_SUBSCRIBE, { scope });
      if (disposed) {
        // Dispose may race an in-flight subscribe. Unsubscribe again after its
        // response so the late registration cannot leak on the App Server. A
        // successor for the same client/scope owns the shared registration.
        await scopeLease.cleanupAfterLateSubscribe(unsubscribeScope);
        return;
      }
      if (subscribeAgain) return;
      state = createSubscriptionIngestionState(result, context);
      refreshing = false;
      retryDelay = SUBSCRIPTION_RETRY_MS;
      applySnapshot(result.snapshot);
      replayPendingEvents();
      if (!subscribeAgain) onBaselineReady?.();
    } catch (error) {
      onBaselineError?.(error);
      scheduleSubscribeRetry();
    } finally {
      subscribeInFlight = false;
      if (subscribeAgain && !disposed) {
        subscribeAgain = false;
        void subscribe();
      }
    }
  }

  function unsubscribeScope() {
    return backendConnection.request(STATE_UNSUBSCRIBE, { scope }).catch(() => {
      // Connection cleanup is best-effort; reconnect/expiry paths also clear stale subscribers.
    });
  }

  function handleStateReset() {
    if (disposed) return;
    if (subscribeInFlight) {
      subscribeAgain = true;
      return;
    }
    void subscribe();
  }

  function scheduleSubscribeRetry() {
    if (disposed || retryTimer !== undefined) return;
    const delay = retryDelay;
    retryDelay = Math.min(retryDelay * 2, MAX_SUBSCRIPTION_RETRY_MS);
    retryTimer = setTimeout(() => {
      retryTimer = undefined;
      void subscribe();
    }, delay);
  }

  function handleEvent(event: AppServerEvent) {
    if (disposed) return;
    if (!state || refreshing) {
      // A subscription snapshot establishes a new cursor baseline. Events that
      // race that read must be replayed only after the baseline is installed.
      queuePendingEvent(event);
      return;
    }
    applyEvent(event);
  }

  function applyEvent(event: AppServerEvent) {
    if (!state || disposed) return false;
    const result = applySubscriptionEvent(state, event);
    if (result.kind === "ignored") {
      // Every listener shares one connection stream. Even events owned by another
      // subscription advance that stream's transport cursor.
      state = result.state;
      return true;
    }
    if (result.kind === "resyncRequired") {
      if (subscribeInFlight) subscribeAgain = true;
      else void subscribe();
      return false;
    }
    state = result.state;
    if (result.snapshotChanged) applySnapshot(result.state.snapshot);
    return true;
  }

  function replayPendingEvents() {
    if (!state || pendingEvents.length === 0) return;
    const snapshotCursorIndex = pendingEvents.findIndex((event) => event.cursor === state?.cursor);
    const events = snapshotCursorIndex === -1
      ? pendingEvents
      : pendingEvents.slice(snapshotCursorIndex + 1);
    pendingEvents = [];
    for (const [index, event] of events.entries()) {
      if (applyEvent(event)) continue;
      for (const remaining of events.slice(index + 1)) queuePendingEvent(remaining);
      break;
    }
  }

  function queuePendingEvent(event: AppServerEvent) {
    if (pendingEvents.length >= MAX_PENDING_EVENTS) {
      // An unbounded queue cannot be reconciled safely. Drop the old generation
      // and require a snapshot taken after the overflow.
      pendingEvents = [];
      if (subscribeInFlight) subscribeAgain = true;
    }
    pendingEvents.push(event);
  }

  function applySnapshot(snapshot: SubscriptionSnapshot) {
    for (const action of actionsFromSubscriptionSnapshot(snapshot, context, {
      currentAgentId,
      dispatch,
      setAgents,
    })) {
      dispatch(action);
    }
  }
}

function acquireSubscriptionLease(
  request: BackendConnection["request"],
  scope: SubscriptionScope,
) {
  const key = subscriptionScopeKey(scope);
  const leases = subscriptionLeases.get(request) ?? new Map<string, SubscriptionLeaseState>();
  subscriptionLeases.set(request, leases);
  const state = leases.get(key) ?? { count: 0 };
  leases.set(key, state);
  state.count += 1;
  let released = false;

  return {
    waitForCleanup() {
      const cleanup = state.cleanup;
      if (!cleanup) return undefined;
      return cleanup.then(() => {
        if (state.cleanup === cleanup) state.cleanup = undefined;
      });
    },
    release(cleanup: () => Promise<unknown>) {
      if (released) return;
      released = true;
      state.count = Math.max(0, state.count - 1);
      if (state.count === 0) void enqueueCleanup(cleanup);
    },
    async cleanupAfterLateSubscribe(cleanup: () => Promise<unknown>) {
      if (state.count > 0) return;
      await enqueueCleanup(cleanup);
    },
  };

  function enqueueCleanup(cleanup: () => Promise<unknown>) {
    // Start the first cleanup immediately. Only successor cleanups need a
    // promise hop to preserve their order behind an older generation.
    const next = (state.cleanup
      ? state.cleanup.catch(() => undefined).then(cleanup)
      : cleanup())
      .catch(() => undefined)
      .then(() => undefined);
    state.cleanup = next;
    void next.then(() => {
      if (state.cleanup === next) state.cleanup = undefined;
    });
    return next;
  }
}

function subscriptionScopeKey(scope: SubscriptionScope) {
  switch (scope.kind) {
    case "projects":
    case "agents":
      return scope.kind;
    case "settings":
      return `settings:${scope.section ?? ""}`;
    case "taskNavigation":
      return `taskNavigation:${scope.projectId ?? ""}`;
    case "task":
      return `task:${scope.taskId}`;
  }
}

function actionsFromSubscriptionSnapshot(
  snapshot: SubscriptionSnapshot,
  context: StateSubscriptionMappingContext,
  agents: {
    currentAgentId?: () => string;
    dispatch: Dispatch<AppAction>;
    setAgents?: (agents: AgentOption[]) => void;
  },
): AppAction[] {
  switch (snapshot.kind) {
    case "projects":
      context.projects = snapshot.projects.projects;
      return [{
        type: "projects",
        activeProjectId: snapshot.projects.activeProjectId ?? undefined,
        projects: snapshot.projects.projects.map((project) => ({
          projectId: project.projectId,
          label: project.label,
        })),
      }];
    case "agents":
      context.agents = snapshot.agents.agents;
      if (agents.setAgents) {
        applyProtocolAgents(snapshot.agents, agents.currentAgentId?.() ?? "", agents.setAgents, agents.dispatch);
      }
      return [];
    case "taskNavigation": {
      const mapped = mapProtocolTaskNavigation(snapshot.navigation, context);
      return [{ type: "tasks", tasks: mapped.tasks }];
    }
    case "task": {
      const mapped = mapProtocolTaskSnapshot(snapshot.task, context);
      return [{ type: "snapshot", snapshot: mapped.snapshot, intent: "refresh" }];
    }
    case "settings":
      return [];
  }
}
