import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type RefObject,
  type SetStateAction,
} from "react";
import type { AppPreferencesRecord } from "@openaide/app-shell-contracts";
import { AppServerProtocolError, type AppServerSession } from "@openaide/app-server-client";
import { refreshAgentSettingsThroughBackend } from "../intents/agentSettingsIntents";
import { refreshSettingsProjectionsThroughBackend } from "../intents/settingsProjectionIntents";
import { startAppServerServerRequestBridge } from "../services/appServerServerRequests";
import {
  mappingContextFromClientSnapshot,
  startAppServerStateSubscription,
  type StateSubscriptionMappingContext,
} from "../services/appServerStateSubscriptions";
import { initializeParamsForBootstrap, taskNavigationScopeForBootstrap } from "../services/backendInitialization";
import {
  openNewTaskSurface,
  openSettingsSurface,
  postHostMessage,
  subscribeHostMessages,
} from "../services/hostBridge";
import { subscribeToBrowserResume } from "../services/browserResume";
import { startHostMessageSession } from "../services/hostMessageSession";
import { agentOptionsFromProtocol } from "../state/appServerAgents";
import { actionsFromInitialSnapshot } from "../state/appServerInitialSnapshot";
import {
  retainedNewTaskContextForInitialization,
  type LiveNewTaskContext,
} from "../state/newTaskSelectionDefaults";
import {
  bindAppServerReplicaEpoch,
  type AppAction,
  type SnapshotIntent,
} from "../state/appReducer";
import type { AgentOption } from "../state/composerOptions";
import { routeHostMessage } from "../state/hostMessageRouter";
import { sendWebviewTelemetry } from "../state/hostMessageTelemetry";
import type { WebviewBootstrap } from "../state/surfaceTypes";
import type { AppState } from "../state/store";
import { shouldLoadTaskNavigation } from "../state/surfaceRouting";
import {
  navigationTargetForBootstrap,
  type AsyncOperationOwner,
} from "../state/asyncOperationOwner";
import { postControllerStarted, postStartupRequests } from "./appControllerEffects";
import { createAppServerInitializeRetry, type AppServerInitializeRetry } from "./appServerInitializeRetry";
import { dispatchStartupReadError, useRoutedBootstrap } from "./appControllerRouting";
import {
  replicaIdentityFromSnapshot,
  useAppServerReplicaLifecycle,
  type AppServerReplicaTransition,
} from "./appServerReplicaLifecycle";
import type { NewTaskController } from "./newTaskController";
import { useNewTaskSubscription } from "./useNewTaskSubscription";
import { useTaskRouteLifecycle } from "./useTaskRouteLifecycle";

export type { AppServerReplicaTransition } from "./appServerReplicaLifecycle";

export type AppControllerBackendConnection = Pick<
  AppServerSession,
  | "initialize"
  | "retryRecovery"
  | "request"
  | "handleNotification"
  | "handleRecoveryBaseline"
  | "handleSessionStatus"
  | "handleResponsiveness"
  | "subscribeState"
  | "handleRequest"
  | "close"
>;

export type BackendConnectionState =
  | { status: "connecting" }
  /** `unresponsive` marks a session that is still open but whose App Server stopped answering. */
  | { status: "ready"; unresponsive?: boolean }
  | { status: "reconnecting"; message: string }
  | { status: "unavailable"; message: string };

type BackendLifecycleOptions = {
  asyncOperations: AsyncOperationOwner;
  backendConnection?: AppControllerBackendConnection;
  currentAgentId: RefObject<string>;
  currentNewTaskContext: RefObject<LiveNewTaskContext>;
  dispatch: Dispatch<AppAction>;
  initialBootstrap: WebviewBootstrap;
  newTaskController: NewTaskController;
  newTaskId?: string;
  onReplicaChanged?: (transition: AppServerReplicaTransition) => void;
  setAgents: Dispatch<SetStateAction<AgentOption[] | undefined>>;
  setNavigationFocusedTaskId: Dispatch<SetStateAction<string | null | undefined>>;
  setPreferences: Dispatch<SetStateAction<AppPreferencesRecord>>;
  state: AppState;
};

export function useAppControllerBackendLifecycle({
  asyncOperations,
  backendConnection,
  currentAgentId,
  currentNewTaskContext,
  dispatch,
  initialBootstrap,
  newTaskController,
  newTaskId,
  onReplicaChanged,
  setAgents,
  setNavigationFocusedTaskId,
  setPreferences,
  state,
}: BackendLifecycleOptions) {
  const operationOwner = asyncOperations;
  const operationOwnerInitialized = useRef(false);
  if (!operationOwnerInitialized.current) {
    operationOwner.observeNavigation(
      navigationTargetForBootstrap(initialBootstrap),
      initialBootstrap.surface === "navigation" ? initialBootstrap.archived === true : undefined,
    );
    operationOwnerInitialized.current = true;
  }
  const { bootstrap, bootstrapRef } = useRoutedBootstrap(
    initialBootstrap,
    operationOwner,
    dispatch,
  );
  const [backendReady, setBackendReady] = useState(false);
  const [backendSessionReady, setBackendSessionReady] = useState(false);
  const [backendConnectionState, setBackendConnectionState] = useState<BackendConnectionState>({
    status: "connecting",
  });
  const [backendUnresponsive, setBackendUnresponsive] = useState(false);
  const [backendStateGeneration, setBackendStateGeneration] = useState(0);
  const {
    dispatchForCurrentReplica,
    establishReplica,
    replicaEpoch,
    replicaEpochRef,
    replicaIdentity,
  } = useAppServerReplicaLifecycle(dispatch, onReplicaChanged);
  const backendInitialized = useRef(false);
  /** Runs a waiting retry of the first initialize at once; false when none is waiting. */
  const retryInitializeNow = useRef<(() => boolean) | undefined>(undefined);
  const [backendInitializationReady, setBackendInitializationReady] = useState(false);
  const stateSubscriptionContext = useRef<StateSubscriptionMappingContext | undefined>(undefined);
  const failedSubscriptionBaselines = useRef(new Map<string, string>());
  const pendingGlobalSubscriptionBaselines = useRef(new Set<string>());

  const createSnapshotRequestId = (taskId?: string, intent: SnapshotIntent = "refresh") => {
    return operationOwner.createSnapshotRequest(taskId, intent);
  };
  const acceptSnapshotRequest = (
    taskId: string,
    requestId: number | undefined,
    intent: SnapshotIntent,
  ) => operationOwner.acceptSnapshot(taskId, requestId, intent);
  const markSubscriptionError = (key: string, error: unknown) => {
    sendWebviewTelemetry(postHostMessage, "app_server_subscription_failed", {
      request: key,
      error_name: errorName(error),
    });
    const message = "App Server is temporarily unavailable.";
    failedSubscriptionBaselines.current.set(key, message);
    setBackendConnectionState({ status: "reconnecting", message });
  };
  const markGlobalSubscriptionLost = (key: string) => {
    const alreadyPending = pendingGlobalSubscriptionBaselines.current.has(key);
    pendingGlobalSubscriptionBaselines.current.add(key);
    if (!alreadyPending) {
      sendWebviewTelemetry(postHostMessage, "app_server_subscription_lost", { request: key });
    }
    setBackendReady(false);
    setBackendConnectionState({
      status: "reconnecting",
      message: "Connection interrupted. Reconnecting automatically.",
    });
  };
  const markSubscriptionReady = (key: string) => {
    const recovered = failedSubscriptionBaselines.current.has(key)
      || pendingGlobalSubscriptionBaselines.current.has(key);
    failedSubscriptionBaselines.current.delete(key);
    pendingGlobalSubscriptionBaselines.current.delete(key);
    if (recovered) {
      sendWebviewTelemetry(postHostMessage, "app_server_subscription_recovered", { request: key });
    }
    const remainingMessage = [...failedSubscriptionBaselines.current.values()].at(-1);
    if (remainingMessage) {
      setBackendConnectionState({ status: "reconnecting", message: remainingMessage });
      return;
    }
    if (pendingGlobalSubscriptionBaselines.current.size > 0) return;
    if (backendInitialized.current) {
      setBackendReady(true);
      setBackendConnectionState({ status: "ready" });
    }
  };

  useEffect(() => {
    if (initialBootstrap.surface === "invalid") return;
    let active = true;
    let initializeRetry: AppServerInitializeRetry | undefined;
    const serverRequestBridge = backendConnection?.handleRequest
      ? startAppServerServerRequestBridge({
          backendConnection: { handleRequest: backendConnection.handleRequest },
          postHostMessage,
        })
      : undefined;
    const stopRecoveryBaselines = backendConnection?.handleRecoveryBaseline((baseline) => {
      if (!active) return;
      sendWebviewTelemetry(postHostMessage, "app_server_recovery_baseline_received", {
        surface: initialBootstrap.surface,
        reason: baseline.reason,
      });
      const recoveredSnapshot = baseline.result.snapshot;
      const recoveredReplicaEpoch = establishReplica(replicaIdentityFromSnapshot(recoveredSnapshot));
      const recoveredDispatch = bindAppServerReplicaEpoch(dispatch, recoveredReplicaEpoch);
      const recoveredContext = mappingContextFromClientSnapshot(recoveredSnapshot);
      const currentContext = stateSubscriptionContext.current;
      if (currentContext) Object.assign(currentContext, recoveredContext);
      else stateSubscriptionContext.current = recoveredContext;

      if (baseline.reason === "clientLivenessExpired" || baseline.reason === "appServerRestarted") {
        const expiredConfigOptions = newTaskController.getSnapshot()?.agent_config;
        const expiredTaskId = newTaskController.expireClientLease();
        if (expiredTaskId) {
          recoveredDispatch({
            type: "newTask:leaseExpired",
            taskId: expiredTaskId,
            message: "Attachment must be reselected after the client session expired.",
            ...(expiredConfigOptions ? { configOptions: expiredConfigOptions } : {}),
          });
        }
      }

      const currentBootstrap = bootstrapRef.current;
      const ingestion = actionsFromInitialSnapshot(recoveredSnapshot, {
        includeActiveTask: currentBootstrap.surface === "task" && Boolean(currentBootstrap.taskId),
        retainedNewTaskContext: retainedNewTaskContextForInitialization(
          recoveredSnapshot,
          currentNewTaskContext.current,
        ),
      });
      for (const action of ingestion.actions) {
        if (action.type === "settings:preferences") setPreferences(action.preferences);
        recoveredDispatch(routeOwnedSnapshotAction(action, currentBootstrap));
      }
      if (recoveredSnapshot.agents) setAgents(agentOptionsFromProtocol(recoveredSnapshot.agents));
      setBackendStateGeneration((current) => current + 1);
      sendWebviewTelemetry(postHostMessage, "app_server_recovery_baseline_applied", {
        surface: initialBootstrap.surface,
        reason: baseline.reason,
      });
    });
    const stopSessionStatus = backendConnection?.handleSessionStatus((next) => {
      if (!active) return;
      sendWebviewTelemetry(postHostMessage, "app_server_session_status_changed", {
        surface: initialBootstrap.surface,
        status: next.status,
        generation: next.generation,
        ...(next.status === "recovering" ? { reason: next.reason } : {}),
        ...(next.status === "unavailable" ? { error_name: errorName(next.error) } : {}),
      });
      if (next.status === "recovering") {
        setBackendSessionReady(false);
        setBackendReady(false);
        setBackendConnectionState({
          status: "reconnecting",
          message: "Connection interrupted. Reconnecting automatically.",
        });
      } else if (next.status === "unavailable") {
        setBackendSessionReady(false);
        setBackendReady(false);
        setBackendConnectionState({
          status: "unavailable",
          message: next.error instanceof Error ? next.error.message : "Unable to restore App Server session.",
        });
      } else if (next.status === "ready" && backendInitialized.current) {
        setBackendSessionReady(true);
        if (
          pendingGlobalSubscriptionBaselines.current.size === 0
          && failedSubscriptionBaselines.current.size === 0
        ) {
          setBackendReady(true);
          setBackendConnectionState({ status: "ready" });
        }
      }
    });
    const stopResponsiveness = backendConnection?.handleResponsiveness?.((responsive) => {
      if (!active) return;
      sendWebviewTelemetry(postHostMessage, "app_server_responsiveness_changed", {
        surface: initialBootstrap.surface,
        responsive,
      });
      setBackendUnresponsive(!responsive);
    });
    const stopSubscriptions: Array<() => void> = [];
    setBackendStateGeneration((generation) => generation + 1);
    backendInitialized.current = false;
    setBackendInitializationReady(false);
    setBackendSessionReady(false);
    failedSubscriptionBaselines.current.clear();
    pendingGlobalSubscriptionBaselines.current.clear();
    setBackendReady(false);
    setBackendConnectionState({ status: "connecting" });
    taskRouteLifecycle.reset();
    if (initialBootstrap.surface === "navigation") {
      const archived = initialBootstrap.archived === true;
      dispatch({ type: "archive:set", showArchived: archived });
    }
    postControllerStarted(postHostMessage, initialBootstrap);
    const stopSession = startHostMessageSession(subscribeHostMessages, (message) => {
      if (serverRequestBridge?.handleHostMessage(message)) return;
      routeHostMessage(message, {
        bootstrap: bootstrapRef.current,
        dispatch: dispatchForCurrentReplica,
        openNewTaskSurface,
        openSettingsSurface,
        setAgents,
        setNavigationFocusedTaskId,
        setPreferences,
        postHostMessage,
      });
    }, () => {
      if (backendConnection) {
        let initializeAttempts = 0;
        const attemptInitialize = () => {
          const initializeStartedAt = Date.now();
          const attempt = ++initializeAttempts;
          sendWebviewTelemetry(postHostMessage, "app_server_initialize_started", {
            surface: initialBootstrap.surface,
            task_id: initialBootstrap.taskId,
            attempt,
          });
          let initializeSucceeded = false;
          void backendConnection
            .initialize(initializeParamsForBootstrap(initialBootstrap))
            .then((result) => {
              initializeSucceeded = true;
              if (!active) return;
              const initializedReplicaEpoch = establishReplica(replicaIdentityFromSnapshot(result.snapshot));
              const initializedDispatch = bindAppServerReplicaEpoch(dispatch, initializedReplicaEpoch);
              initializedDispatch({ type: "appServer:ready" });
              const currentBootstrap = bootstrapRef.current;
              const ingestion = actionsFromInitialSnapshot(result.snapshot, {
                includeActiveTask: currentBootstrap.surface === "task" && Boolean(currentBootstrap.taskId),
                retainedNewTaskContext: retainedNewTaskContextForInitialization(
                  result.snapshot,
                  currentNewTaskContext.current,
                ),
              });
              const subscriptionContext = mappingContextFromClientSnapshot(result.snapshot);
              stateSubscriptionContext.current = subscriptionContext;
              for (const action of ingestion.actions) {
                if (action.type === "settings:preferences") setPreferences(action.preferences);
                if (action.type === "snapshot" && action.snapshot.lifecycle === "prepared") {
                  newTaskController.updateSnapshot(action.snapshot);
                  continue;
                }
                initializedDispatch(routeOwnedSnapshotAction(action, currentBootstrap));
              }
              if (result.snapshot.agents) {
                setAgents(agentOptionsFromProtocol(result.snapshot.agents));
              }
              if (backendConnection) {
                const subscriptionConnection = { subscribeState: backendConnection.subscribeState };
                stopSubscriptions.push(startAppServerStateSubscription({
                  backendConnection: subscriptionConnection,
                  context: subscriptionContext,
                  dispatch: dispatchForCurrentReplica,
                  onBaselineLost: () => markGlobalSubscriptionLost("projects"),
                  onBaselineError: (error) => markSubscriptionError("projects", error),
                  onBaselineReady: () => markSubscriptionReady("projects"),
                  scope: { kind: "projects" },
                }));
                stopSubscriptions.push(startAppServerStateSubscription({
                  backendConnection: subscriptionConnection,
                  context: subscriptionContext,
                  currentAgentId: () => currentAgentId.current,
                  dispatch: dispatchForCurrentReplica,
                  onBaselineLost: () => markGlobalSubscriptionLost("agents"),
                  onBaselineError: (error) => markSubscriptionError("agents", error),
                  onBaselineReady: () => markSubscriptionReady("agents"),
                  scope: { kind: "agents" },
                  setAgents,
                }));
                // Devices never gates readiness: the list only feeds Settings and the new-device notice.
                stopSubscriptions.push(startAppServerStateSubscription({
                  backendConnection: subscriptionConnection,
                  context: subscriptionContext,
                  dispatch: dispatchForCurrentReplica,
                  scope: { kind: "devices" },
                }));
                // Terminals never gate readiness either: only the Terminal page and quit warnings read them.
                stopSubscriptions.push(startAppServerStateSubscription({
                  backendConnection: subscriptionConnection,
                  context: subscriptionContext,
                  dispatch: dispatchForCurrentReplica,
                  scope: { kind: "terminals" },
                }));
                for (const project of result.snapshot.projects?.projects ?? []) {
                  if (!project.worktreeRepositoryId) continue;
                  stopSubscriptions.push(startAppServerStateSubscription({
                    backendConnection: subscriptionConnection,
                    context: subscriptionContext,
                    dispatch: dispatchForCurrentReplica,
                    scope: {
                      kind: "worktreeRepository",
                      repositoryId: project.worktreeRepositoryId,
                    },
                  }));
                }
              }
              if (initialBootstrap.surface === "settings") {
                if (initialBootstrap.settingsTab) {
                  initializedDispatch({ type: "settings:tab", tab: initialBootstrap.settingsTab });
                }
                initializedDispatch({ type: "settings:start" });
                void refreshSettingsProjectionsThroughBackend({
                  backendConnection: { request: backendConnection.request },
                  currentAgentId: currentAgentId.current,
                  dispatch: initializedDispatch,
                  setAgents,
                  state,
                }).catch((error) => {
                  initializedDispatch({
                    type: "settings:error",
                    message: error instanceof Error
                      ? error.message
                      : "Unable to load Agent settings from App Server",
                  });
                });
              }
              // Task surfaces render Agent identity for Tasks whose Agent is no longer in the
              // runtime catalog, and the Settings projection is the only source for a disabled
              // Agent's configured icon. Load it once per client so history keeps it after a
              // restart, not only after Settings was opened.
              void refreshAgentSettingsThroughBackend({
                backendConnection: { request: backendConnection.request },
                currentAgentId: currentAgentId.current,
                dispatch: initializedDispatch,
                setAgents,
                state,
              }).catch(() => undefined);
              // Route opening also starts App Server recovery work, so the route effect must
              // own task/open even when initialize already supplied cached task state.
              backendInitialized.current = true;
              setBackendInitializationReady(true);
              setBackendSessionReady(true);
              sendWebviewTelemetry(postHostMessage, "app_server_initialize_completed", {
                surface: initialBootstrap.surface,
                task_id: initialBootstrap.taskId,
                duration_ms: Date.now() - initializeStartedAt,
                attempt,
              });
              const projectSelection = ingestion.newTaskProjectSelection;
              sendWebviewTelemetry(postHostMessage, "new_task_initial_project_selected", {
                surface: initialBootstrap.surface,
                project_id: projectSelection.projectId,
                selection_source: projectSelection.source,
                client_identity_source: initialBootstrap.clientInstanceId ? "shell" : "web_storage",
                shell_project_present: projectSelection.shellProjectPresent,
                shell_project_valid: projectSelection.shellProjectValid,
                retained_project_present: projectSelection.retainedProjectPresent,
                retained_project_valid: projectSelection.retainedProjectValid,
                default_project_present: projectSelection.defaultProjectPresent,
                default_project_valid: projectSelection.defaultProjectValid,
              });
              const globalBaselinesReady = pendingGlobalSubscriptionBaselines.current.size === 0;
              setBackendReady(globalBaselinesReady);
              if (failedSubscriptionBaselines.current.size === 0 && globalBaselinesReady) {
                setBackendConnectionState({ status: "ready" });
              }
            })
            .catch((error) => {
              if (!active) return;
              sendWebviewTelemetry(postHostMessage, "app_server_initialize_failed", {
                surface: initialBootstrap.surface,
                task_id: initialBootstrap.taskId,
                duration_ms: Date.now() - initializeStartedAt,
                attempt,
                error_name: errorName(error),
                ...(error instanceof AppServerProtocolError ? { error_code: error.protocolError.code } : {}),
              });
              backendInitialized.current = false;
              setBackendInitializationReady(false);
              setBackendSessionReady(false);
              setBackendReady(false);
              const message = error instanceof Error ? error.message : "Unable to connect to App Server.";
              setBackendConnectionState({ status: "unavailable", message });
              dispatch({
                type: "appServer:error",
                message,
              });
              dispatchStartupReadError(bootstrap, dispatch, message);
              // A failure while applying a successful initialize is not retried:
              // the connection keeps that result, and a retry would apply it again.
              if (!initializeSucceeded) initializeRetry?.failed();
            });
        };
        initializeRetry = createAppServerInitializeRetry({
          attempt: attemptInitialize,
          subscribeToResume: subscribeToBrowserResume,
        });
        retryInitializeNow.current = initializeRetry.retryNow;
        attemptInitialize();
      } else {
        sendWebviewTelemetry(postHostMessage, "app_server_connection_unavailable", {
          surface: initialBootstrap.surface,
          reason: "missing_bootstrap_connection",
        });
        setBackendConnectionState({
          status: "unavailable",
          message: "App Server connection unavailable.",
        });
        dispatch({
          type: "appServer:error",
          message: "App Server connection unavailable.",
        });
      }
      postStartupRequests({
        bootstrap: initialBootstrap,
        dispatchNavigationError: (message) => dispatch({ type: "tasks:error", message }),
        dispatchSettingsStart: () => dispatch({ type: "settings:start" }),
        dispatchSettingsError: (message) => dispatch({ type: "settings:error", message }),
        dispatchTaskOpenError: (taskId, message) => dispatch({ type: "taskOpen:error", taskId, message }),
        skipSettingsReadRequests: backendConnection !== undefined,
        skipTaskReadRequests: backendConnection !== undefined,
        postHostMessage,
      });
    });
    return () => {
      active = false;
      initializeRetry?.dispose();
      retryInitializeNow.current = undefined;
      backendInitialized.current = false;
      setBackendInitializationReady(false);
      setBackendSessionReady(false);
      failedSubscriptionBaselines.current.clear();
      pendingGlobalSubscriptionBaselines.current.clear();
      setBackendReady(false);
      taskRouteLifecycle.reset();
      stateSubscriptionContext.current = undefined;
      for (const stop of stopSubscriptions) stop();
      stopRecoveryBaselines?.();
      stopSessionStatus?.();
      stopResponsiveness?.();
      serverRequestBridge?.dispose();
      stopSession();
      backendConnection?.close();
    };
  }, [backendConnection, initialBootstrap.surface, initialBootstrap.taskId]);

  const taskNavigationEnabled = shouldLoadTaskNavigation(bootstrap);
  const taskNavigationSection = state.showArchived ? "archive" : "tasks";
  const taskNavigationScope = taskNavigationScopeForBootstrap(bootstrap, taskNavigationSection);
  // A route change need not change this scope. Re-subscribing asks App Server for
  // another catalog refresh, so compare the generated scope by value, not route identity.
  const taskNavigationScopeKey = JSON.stringify(taskNavigationScope);

  useEffect(() => {
    if (!backendConnection || !backendInitializationReady || !taskNavigationEnabled) return;
    const context = stateSubscriptionContext.current;
    if (!context) return;
    const taskNavigationKey = "task-navigation";
    return startAppServerStateSubscription({
      backendConnection: { subscribeState: backendConnection.subscribeState },
      context,
      dispatch: dispatchForCurrentReplica,
      onBaselineLost: () => markGlobalSubscriptionLost(taskNavigationKey),
      onBaselineError: (error) => {
        if (taskNavigationSection === "archive") {
          dispatchForCurrentReplica({
            type: "tasks:error",
            message: error instanceof Error ? error.message : "Unable to load archived tasks.",
          });
          return;
        }
        markSubscriptionError(taskNavigationKey, error);
      },
      onBaselineReady: () => markSubscriptionReady(taskNavigationKey),
      scope: taskNavigationScope,
    });
  }, [
    backendConnection,
    backendInitializationReady,
    taskNavigationEnabled,
    taskNavigationScopeKey,
    dispatchForCurrentReplica,
  ]);

  useNewTaskSubscription({
    backendConnection,
    backendInitialized,
    backendReady,
    backendStateGeneration,
    context: stateSubscriptionContext,
    dispatch: dispatchForCurrentReplica,
    newTaskController,
    newTaskId,
  });
  const taskRouteLifecycle = useTaskRouteLifecycle({
    acceptSnapshotRequest,
    backendConnection,
    backendInitialized,
    backendReady,
    backendStateGeneration,
    bootstrap,
    createSnapshotRequestId,
    dispatch,
    failedSubscriptionBaselines,
    markSubscriptionError,
    markSubscriptionReady,
    operationOwner,
    replicaEpochRef,
    setBackendConnectionState,
    stateSubscriptionContext,
  });

  // The connection's Retry also starts over a first initialize that failed.
  const routeRetryTaskOpen = taskRouteLifecycle.retryTaskOpen;
  const retryTaskOpen = useCallback(() => {
    if (retryInitializeNow.current?.()) return;
    routeRetryTaskOpen();
  }, [routeRetryTaskOpen]);

  return {
    acceptSnapshotRequest,
    backendInitialized,
    backendInitializationReady,
    backendConnectionState: backendConnectionState.status === "ready" && backendUnresponsive
      ? { status: "ready" as const, unresponsive: true }
      : backendConnectionState,
    backendReady: backendReady && taskRouteLifecycle.ready,
    taskMutationReady: backendSessionReady && taskRouteLifecycle.ready,
    bootstrap,
    createSnapshotRequestId,
    operationOwner,
    replicaEpoch,
    retryTaskOpen,
  };
}

function errorName(error: unknown) {
  return error instanceof Error && error.name ? error.name : typeof error;
}

/** Recovery and initialize snapshots may describe a Task that is no longer routed. */
function routeOwnedSnapshotAction(action: AppAction, bootstrap: WebviewBootstrap): AppAction {
  if (action.type !== "snapshot") return action;
  const routedTaskId = bootstrap.surface === "task" ? bootstrap.taskId : undefined;
  if (!routedTaskId || action.snapshot.task.task_id === routedTaskId) return action;
  return { ...action, intent: "refresh" };
}
