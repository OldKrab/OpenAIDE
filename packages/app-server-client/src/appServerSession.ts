import {
  applySubscriptionEvent,
  createSubscriptionIngestionState,
  type SubscriptionIngestionState,
} from "./stateIngestion.js";
import { subscriptionScopesEqual } from "./subscriptionScope.js";
import {
  CLIENT_HEARTBEAT,
  STATE_SUBSCRIBE,
  STATE_UNSUBSCRIBE,
  type AppServerEvent,
  type InitializeResult,
  type SubscriptionScope,
} from "./generated/protocol.js";
import type {
  AppServerSession,
  AppServerSessionStatus,
  AppServerStateObserver,
  BackendConnection,
  BackendGenerationInvalidation,
  BackendRecoveryBaseline,
  BackendRecoveryFailure,
  BackendUnsubscribe,
} from "./backendConnection.js";

const SUBSCRIPTION_RETRY_MS = 500;
const MAX_SUBSCRIPTION_RETRY_MS = 5_000;
const MAX_SUBSCRIPTION_ATTEMPTS = 5;
const MAX_PENDING_EVENTS = 1_000;

type ScopeReplica = {
  scope: SubscriptionScope;
  observers: Set<AppServerStateObserver>;
  state?: SubscriptionIngestionState;
  pendingEvents: AppServerEvent[];
  refreshing: boolean;
  refreshGeneration?: number;
  refreshPromise?: Promise<void>;
  retryDelay: number;
  cancelRetry?: () => void;
};

type RecoveryGate = {
  generation: number;
  promise: Promise<void>;
  resolve(): void;
};

type ReplicaRefreshCycle = {
  generation: number;
  promise: Promise<void>;
};

/** Deep session module: transport replacement and scope replicas commit behind one readiness gate. */
export function createAppServerSession(connection: BackendConnection): AppServerSession {
  const replicas = new Map<string, ScopeReplica>();
  const statusListeners = new Set<(status: AppServerSessionStatus) => void>();
  const invalidationListeners = new Set<(event: BackendGenerationInvalidation) => void>();
  const recoveryBaselineListeners = new Set<(event: BackendRecoveryBaseline) => void>();
  const recoveryFailureListeners = new Set<(event: BackendRecoveryFailure) => void>();
  let generation = 0;
  let initialized = false;
  let closed = false;
  let recoveryGate: RecoveryGate | undefined;
  let wakeRecoveryPromise: Promise<void> | undefined;
  let replicaRefreshCycle: ReplicaRefreshCycle | undefined;
  let status: AppServerSessionStatus = { status: "connecting", generation };
  let initialization: InitializeResult | undefined;

  const stopEvents = connection.handleNotification("app/event", handleEvent);
  const stopInvalidation = connection.handleGenerationInvalidated(beginRecovery);
  const stopRecoveryBaseline = connection.handleRecoveryBaseline((baseline) => {
    void installRecoveryBaseline(baseline);
  });
  const stopRecoveryFailure = connection.handleRecoveryFailed((failure) => {
    failRecovery(failure);
  });
  const stopWake = connection.handleWake?.(recoverReplicasAfterWake) ?? (() => undefined);

  const session: AppServerSession = {
    async initialize(params, meta) {
      const result = await (meta === undefined
        ? connection.initialize(params)
        : connection.initialize(params, meta));
      initialization = result;
      initialized = true;
      await refreshCurrentReplicas();
      updateStatus({ status: "ready", generation });
      return result;
    },
    async request(method, params, meta) {
      if (recoveryGate || wakeRecoveryPromise) await waitUntilReady();
      if (status.status === "unavailable") throw status.error;
      return meta === undefined
        ? connection.request(method, params)
        : connection.request(method, params, meta);
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
    subscribeState(scope, observer) {
      const key = scopeKey(scope);
      let replica = replicas.get(key);
      if (!replica) {
        replica = {
          scope,
          observers: new Set(),
          pendingEvents: [],
          refreshing: true,
          retryDelay: SUBSCRIPTION_RETRY_MS,
        };
        replicas.set(key, replica);
      }
      replica.observers.add(observer);
      if (replica.state && !replica.refreshing) {
        notifyObserver(observer, "onSnapshot", replica.state.snapshot);
        notifyObserver(observer, "onBaselineReady");
      } else if (initialized && status.status !== "recovering") {
        void refreshReplica(replica, generation).catch((error) => {
          if (!closed) updateStatus({ status: "unavailable", generation, error });
        });
      }
      return () => releaseReplica(key, replica!, observer);
    },
    handleSessionStatus(handler) {
      statusListeners.add(handler);
      notifyListener(handler, status);
      return () => statusListeners.delete(handler);
    },
    close() {
      if (closed) return;
      closed = true;
      recoveryGate?.resolve();
      recoveryGate = undefined;
      stopEvents();
      stopInvalidation();
      stopRecoveryBaseline();
      stopRecoveryFailure();
      stopWake();
      for (const replica of replicas.values()) replica.cancelRetry?.();
      replicas.clear();
      statusListeners.clear();
      invalidationListeners.clear();
      recoveryBaselineListeners.clear();
      recoveryFailureListeners.clear();
      return connection.close();
    },
  };

  return session;

  function beginRecovery(event: BackendGenerationInvalidation) {
    if (closed || recoveryGate) return;
    generation += 1;
    recoveryGate = deferredGate(generation);
    updateStatus({ status: "recovering", generation, reason: event.reason });
    for (const replica of replicas.values()) invalidateReplica(replica);
    notifyListeners(invalidationListeners, event);
  }

  async function installRecoveryBaseline(baseline: BackendRecoveryBaseline) {
    const gate = recoveryGate;
    if (!gate || gate.generation !== generation || closed) return;
    initialization = baseline.result;
    notifyListeners(recoveryBaselineListeners, baseline);
    try {
      await refreshCurrentReplicas();
    } catch (error) {
      failRecovery({ reason: baseline.reason, error });
      return;
    }
    if (closed || recoveryGate !== gate) return;
    recoveryGate = undefined;
    gate.resolve();
    // A wake-triggered liveness request can cause this transport replacement.
    // Keep presentation gated until that wake also renews every active scope.
    if (!wakeRecoveryPromise) updateStatus({ status: "ready", generation });
  }

  function failRecovery(failure: BackendRecoveryFailure) {
    const gate = recoveryGate;
    if (!gate || closed) return;
    recoveryGate = undefined;
    gate.resolve();
    notifyListeners(recoveryFailureListeners, failure);
    updateStatus({ status: "unavailable", generation, error: failure.error });
  }

  function recoverReplicasAfterWake() {
    if (closed || !initialized || wakeRecoveryPromise) return;
    console.info("[OpenAIDE] Browser wake is renewing authoritative state baselines");
    updateStatus({ status: "recovering", generation, reason: "foregroundWake" });
    const attempt = renewAfterWake();
    wakeRecoveryPromise = attempt;
    void attempt.then(
      () => {
        if (wakeRecoveryPromise !== attempt || closed) return;
        wakeRecoveryPromise = undefined;
        if (!recoveryGate) {
          console.info("[OpenAIDE] Browser wake renewed authoritative state baselines");
          updateStatus({ status: "ready", generation });
        }
      },
      (error) => {
        if (wakeRecoveryPromise !== attempt || closed) return;
        wakeRecoveryPromise = undefined;
        console.warn("[OpenAIDE] Browser wake exhausted state baseline recovery", error);
        updateStatus({ status: "unavailable", generation, error });
      },
    );
  }

  async function renewAfterWake() {
    // Prove client liveness even when no state scope is currently observed. The
    // reliable connection will replace an expired physical session if needed.
    const activeRecovery = recoveryGate;
    if (activeRecovery) {
      // The replacement initialization already renews every active scope. A
      // concurrent wake joins that authority instead of starting a second pass.
      await activeRecovery.promise;
      if (status.status === "unavailable") throw status.error;
      return;
    }
    await connection.request(CLIENT_HEARTBEAT, {});
    await refreshCurrentReplicas();
  }

  async function waitUntilReady() {
    while (!closed) {
      const pendingRecovery = recoveryGate?.promise ?? wakeRecoveryPromise;
      if (!pendingRecovery) return;
      await pendingRecovery;
    }
  }

  function refreshCurrentReplicas() {
    const targetGeneration = generation;
    if (replicaRefreshCycle?.generation === targetGeneration) {
      return replicaRefreshCycle.promise;
    }
    const promise = runReplicaRefreshCycle(targetGeneration);
    const cycle = { generation: targetGeneration, promise };
    replicaRefreshCycle = cycle;
    void promise.then(
      () => {
        if (replicaRefreshCycle === cycle) replicaRefreshCycle = undefined;
      },
      () => {
        if (replicaRefreshCycle === cycle) replicaRefreshCycle = undefined;
      },
    );
    return promise;
  }

  async function runReplicaRefreshCycle(targetGeneration: number) {
    while (!closed && generation === targetGeneration) {
      const current = [...replicas.values()].filter((replica) => replica.observers.size > 0);
      await Promise.all(current.map((replica) => refreshReplica(replica, targetGeneration)));
      if (closed || generation !== targetGeneration) return;
      const active = [...replicas.values()].filter((replica) => replica.observers.size > 0);
      if (active.every((replica) => (
        replica.refreshGeneration === targetGeneration
        && !replica.refreshing
      ))) return;
    }
  }

  function refreshReplica(replica: ScopeReplica, targetGeneration: number) {
    if (replica.refreshPromise && replica.refreshGeneration === targetGeneration) {
      return replica.refreshPromise;
    }
    replica.retryDelay = SUBSCRIPTION_RETRY_MS;
    invalidateReplica(replica);
    const refresh = refreshReplicaUntilReady(replica, targetGeneration);
    replica.refreshGeneration = targetGeneration;
    replica.refreshPromise = refresh;
    void refresh.then(
      () => {
        if (replica.refreshPromise === refresh) replica.refreshPromise = undefined;
      },
      () => {
        if (replica.refreshPromise === refresh) replica.refreshPromise = undefined;
      },
    );
    return refresh;
  }

  async function refreshReplicaUntilReady(replica: ScopeReplica, targetGeneration: number) {
    let attempt = 0;
    while (!closed && replica.observers.size > 0 && generation === targetGeneration) {
      attempt += 1;
      try {
        const result = await connection.request(STATE_SUBSCRIBE, { scope: replica.scope });
        if (closed || replica.observers.size === 0 || generation !== targetGeneration) return;
        const snapshot = initialization?.snapshot;
        if (!snapshot) throw new Error("App Server session is not initialized");
        replica.state = createSubscriptionIngestionState(result, {
          stateRootId: snapshot.stateRoot.stateRootId,
          clientInstanceId: snapshot.client.clientInstanceId,
        });
        if (!replayPendingEvents(replica)) {
          // A gap discovered behind this baseline cannot start a concurrent refresh:
          // keep the scope gated and retry at a bounded rate in this same loop.
          replica.state = undefined;
          if (attempt >= MAX_SUBSCRIPTION_ATTEMPTS) {
            throw new Error("Authoritative state baseline remained behind live events");
          }
          await waitForReplicaRetry(replica);
          continue;
        }
        replica.refreshing = false;
        replica.retryDelay = SUBSCRIPTION_RETRY_MS;
        notifyReplica(replica, "onSnapshot", replica.state.snapshot);
        notifyReplica(replica, "onBaselineReady");
        return;
      } catch (error) {
        if (closed || generation !== targetGeneration) return;
        notifyReplica(replica, "onBaselineError", error);
        if (attempt >= MAX_SUBSCRIPTION_ATTEMPTS) throw error;
        await waitForReplicaRetry(replica);
      }
    }
  }

  function invalidateReplica(replica: ScopeReplica) {
    replica.cancelRetry?.();
    replica.cancelRetry = undefined;
    replica.pendingEvents = [];
    if (!replica.refreshing) notifyReplica(replica, "onBaselineLost");
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

  function applyEvent(replica: ScopeReplica, event: AppServerEvent, publish = true) {
    if (!replica.state) return false;
    const result = applySubscriptionEvent(replica.state, event);
    if (result.kind === "ignored") {
      replica.state = result.state;
      return true;
    }
    if (result.kind === "resyncRequired") {
      const refresh = refreshReplica(replica, generation);
      // Refresh invalidation clears obsolete pending events, so retain the live
      // gap only after the replacement subscription has started.
      if (publish) queuePendingEvent(replica, event);
      void refresh.catch((error) => {
        if (!closed) updateStatus({ status: "unavailable", generation, error });
      });
      return false;
    }
    replica.state = result.state;
    if (publish) {
      notifyReplica(replica, "onSnapshot", result.state.snapshot, event, result.snapshotChanged);
    }
    return true;
  }

  function replayPendingEvents(replica: ScopeReplica) {
    if (!replica.state || replica.pendingEvents.length === 0) return true;
    const cursorIndex = replica.pendingEvents.findIndex((event) => event.cursor === replica.state?.cursor);
    const events = cursorIndex === -1 ? replica.pendingEvents : replica.pendingEvents.slice(cursorIndex + 1);
    replica.pendingEvents = [];
    for (const [index, event] of events.entries()) {
      if (applyEvent(replica, event, false)) continue;
      for (const pending of events.slice(index)) queuePendingEvent(replica, pending);
      return false;
    }
    return true;
  }

  function queuePendingEvent(replica: ScopeReplica, event: AppServerEvent) {
    if (replica.pendingEvents.length >= MAX_PENDING_EVENTS) replica.pendingEvents = [];
    replica.pendingEvents.push(event);
  }

  function releaseReplica(key: string, replica: ScopeReplica, observer: AppServerStateObserver) {
    replica.observers.delete(observer);
    if (replica.observers.size > 0 || replicas.get(key) !== replica) return;
    replica.cancelRetry?.();
    replicas.delete(key);
    void connection.request(STATE_UNSUBSCRIBE, { scope: replica.scope }).catch(() => undefined);
  }

  function updateStatus(next: AppServerSessionStatus) {
    status = next;
    notifyListeners(statusListeners, next);
  }

  function waitForReplicaRetry(replica: ScopeReplica) {
    const delay = replica.retryDelay;
    replica.retryDelay = Math.min(replica.retryDelay * 2, MAX_SUBSCRIPTION_RETRY_MS);
    return new Promise<void>((resolve) => {
      let settled = false;
      const finish = () => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        if (replica.cancelRetry === finish) replica.cancelRetry = undefined;
        resolve();
      };
      const timer = setTimeout(finish, delay);
      replica.cancelRetry = finish;
    });
  }
}

function scopeKey(scope: SubscriptionScope) {
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

function deferredGate(generation: number): RecoveryGate {
  let resolve: () => void = () => undefined;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { generation, promise, resolve };
}

function notifyReplica<K extends keyof AppServerStateObserver>(
  replica: ScopeReplica,
  method: K,
  ...args: Parameters<NonNullable<AppServerStateObserver[K]>>
) {
  for (const observer of replica.observers) notifyObserver(observer, method, ...args);
}

function notifyObserver<K extends keyof AppServerStateObserver>(
  observer: AppServerStateObserver,
  method: K,
  ...args: Parameters<NonNullable<AppServerStateObserver[K]>>
) {
  const listener = observer[method] as ((...values: typeof args) => void) | undefined;
  if (listener) notifyListener(listener, ...args);
}

function notifyListeners<T>(listeners: Iterable<(event: T) => void>, event: T) {
  for (const listener of listeners) notifyListener(listener, event);
}

function notifyListener<T extends unknown[]>(listener: (...args: T) => void, ...args: T) {
  try {
    listener(...args);
  } catch (error) {
    console.error("[OpenAIDE] App Server session observer failed", error);
  }
}
