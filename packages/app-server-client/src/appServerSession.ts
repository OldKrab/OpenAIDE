import {
  applySubscriptionEvent,
  installNavigationSubscribeReply,
  installSubscriptionReply,
  type SubscriptionIngestionState,
} from "./stateIngestion.js";
import { entityVersion } from "./stateIngestionEntities.js";
import { subscriptionScopesEqual } from "./subscriptionScope.js";
import {
  NAVIGATION_SUBSCRIBE,
  NAVIGATION_UNSUBSCRIBE,
  NAVIGATION_UPDATE,
  STATE_SUBSCRIBE,
  STATE_UNSUBSCRIBE,
  type AppServerEvent,
  type InitializeResult,
  type NavigationViewParams,
  type NavigationUpdateResult,
  type StateSubscribeResult,
  type SubscriptionScope,
} from "./generated/protocol.js";
import { createDiagnosticsLogger, type DiagnosticsLogger } from "./diagnostics.js";
import type {
  AppServerSession,
  AppServerSessionStatus,
  AppServerStateObserver,
  BackendConnection,
  BackendGenerationInvalidation,
  BackendRecoveryBaseline,
  BackendRecoveryFailure,
  BackendUnsubscribe,
  EntityReplicaSnapshot,
  ReplicaSnapshot,
} from "./backendConnection.js";

const SUBSCRIPTION_RETRY_MS = 500;
const MAX_SUBSCRIPTION_RETRY_MS = 5_000;
const MAX_PENDING_EVENTS = 1_000;
/** Copies kept per entity kind after their last observer leaves (ADR-0086). */
const MAX_RETAINED_COPIES: Record<EntityReplicaSnapshot["kind"], number> = {
  task: 16,
  // Terminal output can be large, so fewer of these are worth keeping.
  toolDetail: 8,
  subagentHistory: 8,
};

type ScopeReplica = {
  scope: SubscriptionScope;
  observers: Set<AppServerStateObserver>;
  state?: SubscriptionIngestionState;
  /**
   * Events that arrive while no baseline is installed: those sent before the subscribe reply,
   * and those after it in the same transport batch, which is delivered before the reply's
   * continuation runs.
   */
  pendingEvents: AppServerEvent[];
  /** A Navigation view's parameters: the latest wanted, and the ones App Server holds. */
  navigation?: { wanted: NavigationViewParams; sent?: NavigationViewParams; pushing: boolean };
  refreshing: boolean;
  refreshGeneration?: number;
  refreshPromise?: Promise<void>;
  retryDelay: number;
};

type RecoveryGate = {
  generation: number;
  startedAt: number;
  promise: Promise<void>;
  resolve(): void;
};

/** Deep session module: transport replacement and scope replicas commit behind one readiness gate. */
export function createAppServerSession(
  connection: BackendConnection,
  logger = createDiagnosticsLogger("openaide-app-server-session"),
): AppServerSession {
  const replicas = new Map<string, ScopeReplica>();
  // Entity copies (Task, Subagent History, Tool detail) kept after their last observer leaves,
  // least recently released first. Returning to the entity offers the copy's content version
  // instead of paying for a baseline (ADR-0086).
  const retainedCopies = new Map<string, EntityReplicaSnapshot>();
  const statusListeners = new Set<(status: AppServerSessionStatus) => void>();
  const invalidationListeners = new Set<(event: BackendGenerationInvalidation) => void>();
  const recoveryBaselineListeners = new Set<(event: BackendRecoveryBaseline) => void>();
  const recoveryFailureListeners = new Set<(event: BackendRecoveryFailure) => void>();
  let generation = 0;
  let initialized = false;
  let closed = false;
  let recoveryGate: RecoveryGate | undefined;
  let status: AppServerSessionStatus = { status: "connecting", generation };
  let initialization: InitializeResult | undefined;
  logger.info("app_server_session_created");

  const stopEvents = connection.handleNotification("app/event", handleEvent);
  const stopInvalidation = connection.handleGenerationInvalidated(beginRecovery);
  const stopRecoveryBaseline = connection.handleRecoveryBaseline((baseline) => {
    void installRecoveryBaseline(baseline);
  });
  const stopRecoveryFailure = connection.handleRecoveryFailed((failure) => {
    failRecovery(failure);
  });

  const session: AppServerSession = {
    ...(connection.retryRecovery ? { retryRecovery: () => connection.retryRecovery!() } : {}),
    ...(connection.probe ? { probe: () => connection.probe!() } : {}),
    async initialize(params, meta) {
      const startedAt = Date.now();
      const initialGeneration = generation;
      // An earlier attempt may have ended unavailable; only a failure during this one counts.
      const statusAtStart = status;
      logger.info("app_server_session_initialize_started", {
        generation,
        has_client_request_id: Boolean(meta?.clientRequestId),
      });
      try {
        const result = await (meta === undefined
          ? connection.initialize(params)
          : connection.initialize(params, meta));
        if (generation === initialGeneration) initialization = result;
        initialized = true;
        await refreshCurrentReplicas(initialGeneration);
        // Initialization can lose its transport while installing observed scopes.
        // Only the current generation may publish readiness or its baseline.
        while (recoveryGate) await recoveryGate.promise;
        if (closed) throw new Error("App Server session closed during initialization");
        if (status.status === "unavailable" && status !== statusAtStart) throw status.error;
        if (generation === initialGeneration) updateStatus({ status: "ready", generation });
        logger.info("app_server_session_initialize_completed", {
          generation,
          duration_ms: Date.now() - startedAt,
          replica_count: replicas.size,
        });
        return initialization ?? result;
      } catch (error) {
        logger.error("app_server_session_initialize_failed", {
          generation,
          duration_ms: Date.now() - startedAt,
          ...diagnosticErrorFields(error),
        });
        throw error;
      }
    },
    async request(method, params, meta) {
      const gate = recoveryGate;
      if (gate) {
        const startedAt = Date.now();
        logger.info("app_server_session_request_waiting_for_recovery", {
          method,
          generation: gate.generation,
        });
        await gate.promise;
        logger.info("app_server_session_request_recovery_wait_completed", {
          method,
          generation: gate.generation,
          duration_ms: Date.now() - startedAt,
        });
      }
      if (method === NAVIGATION_UPDATE) {
        // The session remembers what a view wants, so recovery resubscribes with it.
        const handled = await updateNavigationView(params as NavigationViewParams);
        if (handled) return {} as never;
      }
      return meta === undefined
        ? connection.request(method, params)
        : connection.request(method, params, meta);
    },
    notify(method, params) {
      connection.notify?.(method, params);
    },
    handleRequest(method, handler) {
      return connection.handleRequest(method, handler);
    },
    handleNotification(method, handler) {
      return connection.handleNotification(method, handler);
    },
    handleGenerationInvalidated(handler) {
      invalidationListeners.add(handler);
      return () => invalidationListeners.delete(handler);
    },
    handleRecoveryBaseline(handler) {
      recoveryBaselineListeners.add(handler);
      return () => recoveryBaselineListeners.delete(handler);
    },
    handleRecoveryFailed(handler) {
      recoveryFailureListeners.add(handler);
      return () => recoveryFailureListeners.delete(handler);
    },
    subscribeState(scope, observer, view) {
      const key = scopeKey(scope);
      let replica = replicas.get(key);
      if (scope.kind === "taskNavigation" && !view) {
        throw new Error("A Navigation subscription needs its view parameters");
      }
      if (!replica) {
        replica = {
          scope,
          observers: new Set(),
          ...(view ? { navigation: { wanted: view, pushing: false } } : {}),
          pendingEvents: [],
          refreshing: true,
          retryDelay: SUBSCRIPTION_RETRY_MS,
        };
        replicas.set(key, replica);
        logger.info("app_server_state_subscription_registered", {
          ...scopeDiagnosticFields(scope, key),
          generation,
        });
      }
      replica.observers.add(observer);
      if (view && replica.navigation) {
        // A returning observer brings its own parameters; the view follows the latest.
        replica.navigation.wanted = view;
        void pushNavigationParams(replica);
      }
      if (replica.state && !replica.refreshing) {
        notifyObserver(observer, "onSnapshot", logger, replica.state.snapshot);
        notifyObserver(observer, "onBaselineReady", logger);
      } else if (initialized && status.status !== "recovering") {
        void refreshReplica(replica, generation);
      }
      return () => releaseReplica(key, replica!, observer);
    },
    handleSessionStatus(handler) {
      statusListeners.add(handler);
      notifyListener(handler, logger, "session_status", status);
      return () => statusListeners.delete(handler);
    },
    handleResponsiveness(handler) {
      return connection.handleResponsiveness?.(handler) ?? (() => false);
    },
    close() {
      if (closed) return;
      closed = true;
      logger.info("app_server_session_closed", {
        generation,
        replica_count: replicas.size,
      });
      recoveryGate?.resolve();
      recoveryGate = undefined;
      stopEvents();
      stopInvalidation();
      stopRecoveryBaseline();
      stopRecoveryFailure();
      replicas.clear();
      retainedCopies.clear();
      statusListeners.clear();
      invalidationListeners.clear();
      recoveryBaselineListeners.clear();
      recoveryFailureListeners.clear();
      return connection.close();
    },
  };

  return session;

  function beginRecovery(event: BackendGenerationInvalidation) {
    if (closed) return;
    const previousGate = recoveryGate;
    generation += 1;
    recoveryGate = deferredGate(generation, Date.now());
    // A replacement transport may expire before all of its baselines arrive.
    // Earlier waiters remain blocked until the newest recovery settles.
    if (previousGate) void recoveryGate.promise.then(previousGate.resolve);
    logger.warn("app_server_session_recovery_started", {
      generation,
      reason: event.reason,
      replica_count: replicas.size,
    });
    updateStatus({ status: "recovering", generation, reason: event.reason });
    // A copy stays valid across a transport loss or an App Server restart: it carries the content
    // version it was built at, and App Server answers it with the missed changes, or the whole
    // content when it cannot. Only another state root makes the copies meaningless.
    for (const [key, replica] of replicas) {
      retainCopy(key, replica);
      invalidateReplica(replica);
    }
    notifyListeners(invalidationListeners, event, logger, "generation_invalidation");
  }

  async function installRecoveryBaseline(baseline: BackendRecoveryBaseline) {
    const gate = recoveryGate;
    if (!gate || gate.generation !== generation || closed) return;
    logger.info("app_server_session_recovery_baseline_received", {
      generation,
      reason: baseline.reason,
      replica_count: replicas.size,
      duration_ms: Date.now() - gate.startedAt,
    });
    if (initialization && initialization.snapshot.stateRoot.stateRootId !== baseline.result.snapshot.stateRoot.stateRootId) {
      logger.info("app_server_retained_copies_dropped", {
        generation,
        retained_copy_count: retainedCopies.size,
        reason: "stateRootChanged",
      });
      retainedCopies.clear();
    }
    initialization = baseline.result;
    notifyListeners(recoveryBaselineListeners, baseline, logger, "recovery_baseline");
    await refreshCurrentReplicas(gate.generation);
    if (closed || recoveryGate !== gate) return;
    recoveryGate = undefined;
    gate.resolve();
    updateStatus({ status: "ready", generation });
    logger.info("app_server_session_recovery_completed", {
      generation,
      reason: baseline.reason,
      duration_ms: Date.now() - gate.startedAt,
      replica_count: replicas.size,
    });
  }

  function failRecovery(failure: BackendRecoveryFailure) {
    const gate = recoveryGate;
    if (!gate || closed) return;
    recoveryGate = undefined;
    gate.resolve();
    logger.error("app_server_session_recovery_failed", {
      generation,
      reason: failure.reason,
      duration_ms: Date.now() - gate.startedAt,
      ...diagnosticErrorFields(failure.error),
    });
    notifyListeners(recoveryFailureListeners, failure, logger, "recovery_failure");
    updateStatus({ status: "unavailable", generation, error: failure.error });
  }

  async function refreshCurrentReplicas(targetGeneration = generation) {
    const startedAt = Date.now();
    const observedCount = [...replicas.values()].filter((replica) => replica.observers.size > 0).length;
    logger.info("app_server_state_baselines_refresh_started", {
      generation,
      observed_replica_count: observedCount,
    });
    while (!closed && generation === targetGeneration) {
      // Frontend effects may replace an observed scope while recovery baselines arrive.
      // Converge only stale replicas so a newly observed scope cannot invalidate ready peers.
      const stale = [...replicas.values()].filter((replica) => (
        replica.observers.size > 0
        && (replica.refreshGeneration !== targetGeneration || replica.refreshing)
      ));
      await Promise.all(stale.map((replica) => refreshReplica(replica, targetGeneration)));
      if (closed || generation !== targetGeneration) return;
      const active = [...replicas.values()].filter((replica) => replica.observers.size > 0);
      if (active.every((replica) => (
        replica.refreshGeneration === targetGeneration
        && !replica.refreshing
      ))) {
        logger.info("app_server_state_baselines_refresh_completed", {
          generation,
          duration_ms: Date.now() - startedAt,
          replica_count: active.length,
        });
        return;
      }
    }
  }

  function refreshReplica(replica: ScopeReplica, targetGeneration: number) {
    if (replica.refreshPromise && replica.refreshGeneration === targetGeneration) {
      return replica.refreshPromise;
    }
    invalidateReplica(replica);
    const refresh = refreshReplicaUntilReady(replica, targetGeneration);
    replica.refreshGeneration = targetGeneration;
    replica.refreshPromise = refresh;
    void refresh.finally(() => {
      if (replica.refreshPromise === refresh) replica.refreshPromise = undefined;
    });
    return refresh;
  }

  async function refreshReplicaUntilReady(replica: ScopeReplica, targetGeneration: number) {
    let attempt = 0;
    // Only the first attempt offers a retained copy; a rejected resume falls back to a baseline.
    let retained = takeRetainedCopy(scopeKey(replica.scope));
    while (!closed && replica.observers.size > 0 && generation === targetGeneration) {
      attempt += 1;
      const startedAt = Date.now();
      const knownVersion = retained ? entityVersion(retained) : undefined;
      try {
        logger.info("app_server_state_baseline_request_started", {
          ...scopeDiagnosticFields(replica.scope, scopeKey(replica.scope)),
          generation: targetGeneration,
          attempt,
          known_version_offered: knownVersion !== undefined,
        });
        const navigation = replica.navigation;
        const sentParams = navigation?.wanted;
        const result = navigation && sentParams
          ? await connection.request(NAVIGATION_SUBSCRIBE, sentParams)
          : await connection.request(
            STATE_SUBSCRIBE,
            knownVersion === undefined ? { scope: replica.scope } : { scope: replica.scope, knownVersion },
          );
        if (closed || replica.observers.size === 0 || generation !== targetGeneration) return;
        const snapshot = initialization?.snapshot;
        if (!snapshot) throw new Error("App Server session is not initialized");
        const offered = retained;
        retained = undefined;
        const context = {
          stateRootId: snapshot.stateRoot.stateRootId,
          clientInstanceId: snapshot.client.clientInstanceId,
        };
        const state = "frame" in result
          ? installNavigationSubscribeReply(result, replica.scope, context)
          : installSubscriptionReply(result, context, offered);
        if (navigation && state) navigation.sent = sentParams;
        if (!state) {
          logger.warn("app_server_state_resume_rejected", {
            ...scopeDiagnosticFields(replica.scope, scopeKey(replica.scope)),
            generation: targetGeneration,
            attempt,
            duration_ms: Date.now() - startedAt,
          });
          continue;
        }
        replica.state = state;
        replica.refreshing = false;
        replica.retryDelay = SUBSCRIPTION_RETRY_MS;
        notifyReplica(replica, "onSnapshot", logger, replica.state.snapshot);
        const droppedPendingCount = replayPendingEvents(replica);
        // Replay can invalidate the baseline while this refresh still owns the
        // promise. Keep that owner running until the buffered stream is coherent.
        if (replica.refreshing) continue;
        notifyReplica(replica, "onBaselineReady", logger);
        // Parameters that changed while the baseline was in flight go out now.
        void pushNavigationParams(replica);
        logger.info("app_server_state_baseline_ready", {
          ...scopeDiagnosticFields(replica.scope, scopeKey(replica.scope)),
          generation: targetGeneration,
          attempt,
          reply_kind: "frame" in result ? "navigationFrame" : result.snapshot.kind,
          ...("frame" in result ? {} : replySyncDiagnosticFields(result.snapshot)),
          dropped_pending_event_count: droppedPendingCount,
          duration_ms: Date.now() - startedAt,
        });
        return;
      } catch (error) {
        if (closed || generation !== targetGeneration) return;
        const delay = replica.retryDelay;
        logger.warn("app_server_state_baseline_failed", {
          ...scopeDiagnosticFields(replica.scope, scopeKey(replica.scope)),
          generation: targetGeneration,
          attempt,
          retry_delay_ms: delay,
          duration_ms: Date.now() - startedAt,
          ...diagnosticErrorFields(error),
        });
        notifyReplica(replica, "onBaselineError", logger, error);
        replica.retryDelay = Math.min(replica.retryDelay * 2, MAX_SUBSCRIPTION_RETRY_MS);
        await wait(delay);
      }
    }
  }

  function invalidateReplica(replica: ScopeReplica) {
    replica.pendingEvents = [];
    if (!replica.refreshing) notifyReplica(replica, "onBaselineLost", logger);
    replica.refreshing = true;
    replica.state = undefined;
  }

  function handleEvent(event: AppServerEvent) {
    if (closed) return;
    for (const replica of replicas.values()) {
      if (!subscriptionScopesEqual(replica.scope, event.subscription)) continue;
      if (!replica.state || replica.refreshing) {
        queuePendingEvent(replica, event);
        continue;
      }
      applyEvent(replica, event);
    }
  }

  function applyEvent(replica: ScopeReplica, event: AppServerEvent) {
    if (!replica.state) return false;
    const result = applySubscriptionEvent(replica.state, event);
    if (result.kind === "ignored") {
      replica.state = result.state;
      return true;
    }
    if (result.kind === "resyncRequired") {
      if (result.reason === "cursorGap") {
        // Each subscription's chain links every event App Server sent it, in wire order, so a
        // gap means an event was lost or reordered: a bug, not a race. Resync still recovers.
        logger.error("app_server_state_cursor_gap", {
          ...scopeDiagnosticFields(replica.scope, scopeKey(replica.scope)),
          generation,
          expected_previous_cursor: replica.state.cursor,
          previous_cursor: event.previousCursor,
          cursor: event.cursor,
        });
      } else {
        logger.warn("app_server_state_event_rejected", {
          ...scopeDiagnosticFields(replica.scope, scopeKey(replica.scope)),
          generation,
          reason: result.reason,
        });
      }
      invalidateReplica(replica);
      void refreshReplica(replica, generation);
      return false;
    }
    replica.state = result.state;
    notifyReplica(
      replica,
      "onSnapshot",
      logger,
      result.state.snapshot,
      event,
      result.snapshotChanged,
    );
    return true;
  }

  /**
   * Applies the events buffered while the baseline was in flight and returns how many it
   * dropped. App Server sends events in publish order and sends a subscription's earlier events
   * before its subscribe reply, so the first event after the baseline links to the baseline's
   * cursor; every buffered event before it is already in the baseline.
   */
  function replayPendingEvents(replica: ScopeReplica) {
    if (!replica.state || replica.pendingEvents.length === 0) return 0;
    const baseline = replica.state.cursor;
    const firstAfterBaseline = replica.pendingEvents.findIndex((event) => event.previousCursor === baseline);
    const events = firstAfterBaseline === -1 ? [] : replica.pendingEvents.slice(firstAfterBaseline);
    const dropped = replica.pendingEvents.length - events.length;
    replica.pendingEvents = [];
    for (const [index, event] of events.entries()) {
      if (applyEvent(replica, event)) continue;
      for (const pending of events.slice(index + 1)) queuePendingEvent(replica, pending);
      break;
    }
    return dropped;
  }

  function queuePendingEvent(replica: ScopeReplica, event: AppServerEvent) {
    if (replica.pendingEvents.length >= MAX_PENDING_EVENTS) replica.pendingEvents = [];
    replica.pendingEvents.push(event);
  }

  function releaseReplica(key: string, replica: ScopeReplica, observer: AppServerStateObserver) {
    replica.observers.delete(observer);
    if (replica.observers.size > 0 || replicas.get(key) !== replica) return;
    replicas.delete(key);
    retainCopy(key, replica);
    logger.info("app_server_state_subscription_released", {
      ...scopeDiagnosticFields(replica.scope, key),
      generation,
    });
    const released = replica.scope.kind === "taskNavigation"
      ? connection.request(NAVIGATION_UNSUBSCRIBE, { section: replica.scope.section })
      : connection.request(STATE_UNSUBSCRIBE, { scope: replica.scope });
    void released.catch(() => undefined);
  }

  /**
   * Remembers the parameters a Navigation view now wants and returns whether this session owns
   * that view (so the request is answered here, not sent as-is).
   */
  async function updateNavigationView(params: NavigationViewParams) {
    const replica = replicas.get(scopeKey({ kind: "taskNavigation", section: params.section }));
    if (!replica?.navigation) return false;
    replica.navigation.wanted = params;
    await pushNavigationParams(replica);
    return true;
  }

  /**
   * Sends the wanted parameters when App Server holds others. One push runs at a time and
   * re-reads the wanted parameters when it finishes, so a burst of changes costs few requests.
   * A view that is not ready sends nothing: its next baseline carries the latest parameters.
   */
  async function pushNavigationParams(replica: ScopeReplica) {
    const navigation = replica.navigation;
    if (!navigation || navigation.pushing) return;
    navigation.pushing = true;
    const startedAt = Date.now();
    let attempts = 0;
    try {
      while (
        !closed
        && replica.state
        && !replica.refreshing
        && JSON.stringify(navigation.sent) !== JSON.stringify(navigation.wanted)
      ) {
        const params = navigation.wanted;
        attempts += 1;
        await connection.request(NAVIGATION_UPDATE, params) satisfies NavigationUpdateResult;
        navigation.sent = params;
      }
      if (attempts > 0) {
        logger.info("app_server_navigation_update_completed", {
          ...scopeDiagnosticFields(replica.scope, scopeKey(replica.scope)),
          generation,
          attempt: attempts,
          duration_ms: Date.now() - startedAt,
        });
      }
    } catch (error) {
      logger.warn("app_server_navigation_update_failed", {
        ...scopeDiagnosticFields(replica.scope, scopeKey(replica.scope)),
        generation,
        attempt: attempts,
        duration_ms: Date.now() - startedAt,
        ...diagnosticErrorFields(error),
      });
      // The view's state on App Server is unknown now; a fresh baseline settles it.
      if (!closed && replicas.get(scopeKey(replica.scope)) === replica) {
        invalidateReplica(replica);
        void refreshReplica(replica, generation);
      }
    } finally {
      navigation.pushing = false;
    }
  }

  function retainCopy(key: string, replica: ScopeReplica) {
    const snapshot = replica.state?.snapshot;
    if (replica.refreshing || !isEntityReplica(snapshot)) return;
    retainedCopies.delete(key);
    retainedCopies.set(key, snapshot);
    // Evict the least recently released copy of the same kind once its kind is over its limit.
    let sameKind = 0;
    for (const copy of retainedCopies.values()) if (copy.kind === snapshot.kind) sameKind += 1;
    for (const [oldestKey, copy] of retainedCopies) {
      if (sameKind <= MAX_RETAINED_COPIES[snapshot.kind]) break;
      if (copy.kind !== snapshot.kind) continue;
      retainedCopies.delete(oldestKey);
      sameKind -= 1;
    }
  }

  function takeRetainedCopy(key: string) {
    const snapshot = retainedCopies.get(key);
    retainedCopies.delete(key);
    return snapshot;
  }

  function updateStatus(next: AppServerSessionStatus) {
    if (status.status !== next.status || status.generation !== next.generation) {
      logger.info("app_server_session_status_changed", {
        previous_status: status.status,
        status: next.status,
        generation: next.generation,
        ...(next.status === "recovering" ? { reason: next.reason } : {}),
      });
    }
    status = next;
    notifyListeners(statusListeners, next, logger, "session_status");
  }
}

function isEntityReplica(snapshot: ReplicaSnapshot | undefined): snapshot is EntityReplicaSnapshot {
  return snapshot?.kind === "task" || snapshot?.kind === "toolDetail" || snapshot?.kind === "subagentHistory";
}

/** How an entity reply brought the client's copy current; empty for other scopes. */
function replySyncDiagnosticFields(snapshot: StateSubscribeResult["snapshot"]) {
  switch (snapshot.kind) {
    case "task":
      return {
        content_sync: snapshot.chatSync.kind,
        missed_change_count: snapshot.chatSync.kind === "deltas" ? snapshot.chatSync.changes.length : 0,
      };
    case "toolDetail":
      return {
        content_sync: snapshot.terminalSync.kind,
        missed_change_count: snapshot.terminalSync.kind === "deltas" ? snapshot.terminalSync.appends.length : 0,
      };
    case "subagentHistory":
      return { content_sync: snapshot.chatSync.kind, missed_change_count: 0 };
    default:
      return {};
  }
}

function scopeDiagnosticFields(scope: SubscriptionScope, key: string) {
  return {
    scope_kind: scope.kind,
    scope_key: key,
  };
}

function diagnosticErrorFields(error: unknown) {
  return {
    error_kind: error instanceof Error && error.name ? error.name : typeof error,
  };
}

function scopeKey(scope: SubscriptionScope) {
  switch (scope.kind) {
    case "projects":
    case "agents":
    case "devices":
    case "terminals":
      return scope.kind;
    case "settings":
      return `settings:${scope.section ?? ""}`;
    case "taskNavigation":
      return `taskNavigation:${scope.section}`;
    case "task":
      return `task:${scope.taskId}`;
    case "subagentCatalog":
      return `subagentCatalog:${scope.taskId}`;
    case "subagentHistory":
      return `subagentHistory:${scope.taskId}:${scope.subagentId}`;
    case "toolDetail":
      return `toolDetail:${scope.taskId}:${scope.artifactId}`;
    case "worktreeRepository":
      return `worktreeRepository:${scope.repositoryId}`;
  }
}

function deferredGate(generation: number, startedAt: number): RecoveryGate {
  let resolve: () => void = () => undefined;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { generation, startedAt, promise, resolve };
}

function wait(delay: number) {
  return new Promise<void>((resolve) => setTimeout(resolve, delay));
}

function notifyReplica<K extends keyof AppServerStateObserver>(
  replica: ScopeReplica,
  method: K,
  logger: DiagnosticsLogger,
  ...args: Parameters<NonNullable<AppServerStateObserver[K]>>
) {
  for (const observer of replica.observers) {
    notifyObserver(observer, method, logger, ...args);
  }
}

function notifyObserver<K extends keyof AppServerStateObserver>(
  observer: AppServerStateObserver,
  method: K,
  logger: DiagnosticsLogger,
  ...args: Parameters<NonNullable<AppServerStateObserver[K]>>
) {
  const listener = observer[method] as ((...values: typeof args) => void) | undefined;
  if (listener) notifyListener(listener, logger, String(method), ...args);
}

function notifyListeners<T>(
  listeners: Iterable<(event: T) => void>,
  event: T,
  logger: DiagnosticsLogger,
  listenerKind: string,
) {
  for (const listener of listeners) notifyListener(listener, logger, listenerKind, event);
}

function notifyListener<T extends unknown[]>(
  listener: (...args: T) => void,
  logger: DiagnosticsLogger,
  listenerKind: string,
  ...args: T
) {
  try {
    listener(...args);
  } catch (error) {
    logger.error("app_server_session_listener_failed", {
      listener_kind: listenerKind,
      error_kind: error instanceof Error && error.name ? error.name : typeof error,
    });
  }
}
