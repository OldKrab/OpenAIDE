import {
  applySubscriptionEvent,
  createSubscriptionIngestionState,
  subscriptionScopesEqual,
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
  type TaskNavigationSnapshot,
} from "@openaide/app-server-client";
import type { Dispatch } from "react";
import type { AppAction } from "../state/appReducer";
import { applyProtocolAgents } from "../state/appServerAgents";
import {
  createProtocolTaskSnapshotMapper,
  mapProtocolTaskNavigation,
} from "../state/appServerProtocolMapping";
import { mapProtocolToolDetail } from "../state/appServerProtocolChatMapping";
import type { AgentOption } from "../state/composerOptions";

type StateSubscriptionConnection = Pick<BackendConnection, "handleNotification" | "request">;
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
  taskNavigation?: TaskNavigationSnapshot;
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
  const mapTaskSnapshot = createProtocolTaskSnapshotMapper();
  const scopeLease = acquireSubscriptionLease(backendConnection.request, scope);
  let disposed = false;
  let state: SubscriptionIngestionState | undefined;
  let pendingEvents: AppServerEvent[] = [];
  let subscribeInFlight = false;
  let subscribeAgain = false;
  let refreshing = true;
  let retryTimer: ReturnType<typeof setTimeout> | undefined;
  let retryDelay = SUBSCRIPTION_RETRY_MS;
  const unsubscribe = backendConnection.handleNotification("app/event", handleEvent);

  void subscribe();

  return () => {
    disposed = true;
    if (retryTimer !== undefined) clearTimeout(retryTimer);
    unsubscribe();
    scopeLease.release(unsubscribeScope);
  };

  async function subscribe() {
    if (subscribeInFlight) {
      subscribeAgain = true;
      return;
    }
    subscribeInFlight = true;
    if (retryTimer !== undefined) clearTimeout(retryTimer);
    retryTimer = undefined;
    if (!refreshing) onBaselineLost?.();
    refreshing = true;
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
      applySnapshot(state.snapshot);
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
    applyEvent(event, true);
  }

  function applyEvent(event: AppServerEvent, presentLive: boolean) {
    if (!state || disposed) return false;
    const result = applySubscriptionEvent(state, event);
    if (result.kind === "ignored") {
      state = result.state;
      return true;
    }
    if (result.kind === "resyncRequired") {
      void subscribe();
      return false;
    }
    state = result.state;
    const liveText = presentLive
      ? liveTextPresentationAction(event, result.state.snapshot)
      : undefined;
    if (result.snapshotChanged) {
      applySnapshot(result.state.snapshot, liveText);
    } else if (liveText) {
      dispatch(liveText);
    }
    return true;
  }

  function replayPendingEvents() {
    if (!state || pendingEvents.length === 0) return;
    const scopedEvents = pendingEvents.filter((event) => (
      subscriptionScopesEqual(event.subscription, scope)
    ));
    const snapshotCursorIndex = scopedEvents.findIndex((event) => event.cursor === state?.cursor);
    const events = snapshotCursorIndex === -1
      ? scopedEvents
      : scopedEvents.slice(snapshotCursorIndex + 1);
    pendingEvents = [];
    for (const [index, event] of events.entries()) {
      if (applyEvent(event, false)) continue;
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

  function applySnapshot(
    snapshot: SubscriptionSnapshot,
    liveText?: Extract<AppAction, { type: "taskChat:liveText" }>,
  ) {
    for (const action of actionsFromSubscriptionSnapshot(snapshot, context, {
      currentAgentId,
      dispatch,
      setAgents,
    }, mapTaskSnapshot)) {
      if (
        action.type === "snapshot"
        && liveText
        && action.snapshot.task.task_id === liveText.taskId
      ) {
        dispatch({
          ...action,
          liveText: {
            messageId: liveText.messageId,
            channel: liveText.channel,
            eventCursor: liveText.eventCursor,
          },
        });
      } else {
        dispatch(action);
      }
    }
  }
}

function liveTextPresentationAction(
  event: AppServerEvent,
  snapshot: SubscriptionSnapshot,
): Extract<AppAction, { type: "taskChat:liveText" }> | undefined {
  if (snapshot.kind !== "task") return undefined;
  const payload = event.payload;
  if (payload.kind !== "taskChanged") return undefined;
  const liveChange = [...(payload.changes.chat ?? [])].reverse().find((change) => (
    change.kind === "append" || change.kind === "appendText"
  ));
  if (!liveChange) return undefined;
  const messageId = liveChange.kind === "append" ? liveChange.item.messageId : liveChange.messageId;
  const item = liveChange.kind === "append"
    ? liveChange.item
    : snapshot.task.chat.items.find((candidate) => candidate.messageId === messageId);
  const channel = item && textChannel(item);
  if (!channel) return undefined;
  return {
    type: "taskChat:liveText",
    taskId: payload.taskId,
    messageId,
    channel,
    eventCursor: event.cursor,
  };
}

function textChannel(item: import("@openaide/app-server-client").ChatItem) {
  if (!item.parts.some((part) => part.kind === "text")) return undefined;
  if (item.role === "agent") return "agent" as const;
  if (item.role === "system") return "thought" as const;
  return undefined;
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
    case "toolDetail":
      return `toolDetail:${scope.taskId}:${scope.artifactId}`;
    case "worktreeRepository":
      return `worktreeRepository:${scope.repositoryId}`;
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
  mapTaskSnapshot = createProtocolTaskSnapshotMapper(),
): AppAction[] {
  switch (snapshot.kind) {
    case "projects":
      context.projects = snapshot.projects.projects;
      return [{
        type: "projects",
        projects: snapshot.projects.projects.map((project) => ({
          projectId: project.projectId,
          label: project.label,
          workspaceRoot: project.workspaceRoot,
          available: project.available,
          worktreeRepositoryId: project.worktreeRepositoryId ?? undefined,
          projectWorktreeId: project.projectWorktreeId ?? undefined,
          worktreeError: project.worktreeError ?? undefined,
        })),
      }, ...remappedTaskNavigationActions(context)];
    case "agents":
      context.agents = snapshot.agents.agents;
      if (agents.setAgents) {
        applyProtocolAgents(snapshot.agents, agents.currentAgentId?.() ?? "", agents.setAgents, agents.dispatch);
      }
      return remappedTaskNavigationActions(context);
    case "taskNavigation": {
      context.taskNavigation = snapshot.navigation;
      return remappedTaskNavigationActions(context);
    }
    case "task": {
      const mapped = mapTaskSnapshot(snapshot.task, context);
      return [{ type: "snapshot", snapshot: mapped.snapshot, intent: "refresh" }];
    }
    case "settings":
      return [];
    case "toolDetail":
      return [{
        type: "toolDetail:result",
        taskId: snapshot.taskId,
        artifactId: snapshot.artifactId,
        details: mapProtocolToolDetail(snapshot.details),
      }];
    case "worktreeRepository":
      return [{ type: "worktreeRepository", repository: snapshot.repository }];
  }
}

function remappedTaskNavigationActions(context: StateSubscriptionMappingContext): AppAction[] {
  if (!context.taskNavigation) return [];
  const mapped = mapProtocolTaskNavigation(context.taskNavigation, context);
  return [{ type: "tasks", archived: false, tasks: mapped.tasks }];
}
