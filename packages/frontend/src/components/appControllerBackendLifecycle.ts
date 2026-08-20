import {
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type RefObject,
  type SetStateAction,
} from "react";
import type { AppPreferencesRecord } from "@openaide/app-shell-contracts";
import type { AppServerSession } from "@openaide/app-server-client";
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
  | "request"
  | "handleNotification"
  | "handleRecoveryBaseline"
  | "handleSessionStatus"
  | "subscribeState"
  | "handleRequest"
  | "close"
>;

export type BackendConnectionState =
  | { status: "connecting" }
  | { status: "ready" }
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
  const [backendStateGeneration, setBackendStateGeneration] = useState(0);
  const {
    dispatchForCurrentReplica,
    establishReplica,
    replicaEpoch,
    replicaEpochRef,
    replicaIdentity,
  } = useAppServerReplicaLifecycle(dispatch, onReplicaChanged);
  const backendInitialized = useRef(false);
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

      if (baseline.reason === "clientLivenessExpired") {
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
        const initializeStartedAt = Date.now();
        sendWebviewTelemetry(postHostMessage, "app_server_initialize_started", {
          surface: initialBootstrap.surface,
          task_id: initialBootstrap.taskId,
        });
        void backendConnection
          .initialize(initializeParamsForBootstrap(initialBootstrap))
          .then((result) => {
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
            // Route opening also starts App Server recovery work, so the route effect must
            // own task/open even when initialize already supplied cached task state.
            backendInitialized.current = true;
            setBackendInitializationReady(true);
            setBackendSessionReady(true);
            sendWebviewTelemetry(postHostMessage, "app_server_initialize_completed", {
              surface: initialBootstrap.surface,
              task_id: initialBootstrap.taskId,
              duration_ms: Date.now() - initializeStartedAt,
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
              error_name: errorName(error),
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
          });
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
      serverRequestBridge?.dispose();
      stopSession();
      backendConnection?.close();
    };
  }, [backendConnection, initialBootstrap.surface, initialBootstrap.taskId]);

  useEffect(() => {
    if (!backendConnection || !backendInitializationReady || !shouldLoadTaskNavigation(bootstrap)) return;
    const context = stateSubscriptionContext.current;
    if (!context) return;
    const section = state.showArchived ? "archive" : "tasks";
    const taskNavigationKey = "task-navigation";
    return startAppServerStateSubscription({
      backendConnection: { subscribeState: backendConnection.subscribeState },
      context,
      dispatch: dispatchForCurrentReplica,
      onBaselineLost: () => markGlobalSubscriptionLost(taskNavigationKey),
      onBaselineError: (error) => {
        if (section === "archive") {
          dispatchForCurrentReplica({
            type: "tasks:error",
            message: error instanceof Error ? error.message : "Unable to load archived tasks.",
          });
          return;
        }
        markSubscriptionError(taskNavigationKey, error);
      },
      onBaselineReady: () => markSubscriptionReady(taskNavigationKey),
      scope: taskNavigationScopeForBootstrap(bootstrap, section),
    });
  }, [
    backendConnection,
    backendInitializationReady,
    bootstrap,
    dispatchForCurrentReplica,
    state.showArchived,
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

  return {
    acceptSnapshotRequest,
    backendInitialized,
    backendInitializationReady,
    backendConnectionState,
    backendReady: backendReady && taskRouteLifecycle.ready,
    taskMutationReady: backendSessionReady && taskRouteLifecycle.ready,
    bootstrap,
    createSnapshotRequestId,
    operationOwner,
    replicaEpoch,
    retryTaskOpen: taskRouteLifecycle.retryTaskOpen,
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
