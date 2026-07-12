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
import type { BackendConnection, TaskId } from "@openaide/app-server-client";
import { requestMissingInitialTaskList, requestTaskOpen } from "../intents/taskReadIntents";
import { refreshSettingsProjectionsThroughBackend } from "../intents/settingsProjectionIntents";
import { startAppServerServerRequestBridge } from "../services/appServerServerRequests";
import {
  mappingContextFromClientSnapshot,
  startAppServerStateSubscription,
  type StateSubscriptionMappingContext,
} from "../services/appServerStateSubscriptions";
import { initializeParamsForBootstrap, taskNavigationScopeForBootstrap } from "../services/backendInitialization";
import { postHostMessage, subscribeHostMessages } from "../services/hostBridge";
import { startHostMessageSession } from "../services/hostMessageSession";
import { applyProtocolAgents } from "../state/appServerAgents";
import { actionsFromInitialSnapshot } from "../state/appServerInitialSnapshot";
import type { AppAction, SnapshotIntent } from "../state/appReducer";
import type { AgentOption } from "../state/composerOptions";
import { routeHostMessage } from "../state/hostMessageRouter";
import type { WebviewBootstrap } from "../state/surfaceTypes";
import type { AppState } from "../state/store";
import { postControllerStarted, postStartupRequests } from "./appControllerEffects";
import type { AppControllerRefs } from "./appControllerRefs";
import { dispatchStartupReadError, useRoutedBootstrap } from "./appControllerRouting";

export type AppControllerBackendConnection = Pick<
  BackendConnection,
  "initialize" | "request" | "respond" | "serverRequests" | "close"
> & Partial<Pick<BackendConnection, "events" | "stateResets">>;

export type BackendConnectionState =
  | { status: "connecting" }
  | { status: "ready" }
  | { status: "reconnecting"; message: string }
  | { status: "unavailable"; message: string };

type BackendLifecycleOptions = {
  backendConnection?: AppControllerBackendConnection;
  currentAgentId: RefObject<string>;
  dispatch: Dispatch<AppAction>;
  initialBootstrap: WebviewBootstrap;
  refs: AppControllerRefs;
  setAgents: Dispatch<SetStateAction<AgentOption[] | undefined>>;
  setPreferences: Dispatch<SetStateAction<AppPreferencesRecord>>;
  state: AppState;
};

export function useAppControllerBackendLifecycle({
  backendConnection,
  currentAgentId,
  dispatch,
  initialBootstrap,
  refs,
  setAgents,
  setPreferences,
  state,
}: BackendLifecycleOptions) {
  const { bootstrap, bootstrapRef } = useRoutedBootstrap(initialBootstrap, dispatch);
  const [backendReady, setBackendReady] = useState(false);
  const [backendConnectionState, setBackendConnectionState] = useState<BackendConnectionState>({
    status: "connecting",
  });
  const [backendStateGeneration, setBackendStateGeneration] = useState(0);
  const [readyTaskSubscriptionKey, setReadyTaskSubscriptionKey] = useState<string | undefined>();
  const [readyRouteOpenKey, setReadyRouteOpenKey] = useState<string | undefined>();
  const [routeOpenSettlement, setRouteOpenSettlement] = useState(0);
  const backendStateGenerationRef = useRef(0);
  const backendInitialized = useRef(false);
  const lastRequestedRouteTaskKey = useRef<string | undefined>(undefined);
  const routeOpenInFlight = useRef<{ promise: Promise<void>; taskId: string } | undefined>(undefined);
  const routeOpenError = useRef<string | undefined>(undefined);
  const navigationGeneration = useRef(0);
  const stateSubscriptionContext = useRef<StateSubscriptionMappingContext | undefined>(undefined);
  const failedSubscriptionBaselines = useRef(new Map<string, string>());
  const {
    latestNativeSessionSelection,
    latestOptionsRequestKey,
    latestSessionListRequestId,
    nextSessionListRequestId,
    snapshotRequests,
  } = refs;

  const createSnapshotRequestId = (taskId?: string, intent: SnapshotIntent = "refresh") => {
    return snapshotRequests.current.create(taskId, intent);
  };
  const acceptSnapshotRequest = (
    taskId: string,
    requestId: number | undefined,
    intent: SnapshotIntent,
  ) => snapshotRequests.current.accept(taskId, requestId, intent).accepted;
  const beginNavigationChange = (archived?: boolean) => {
    navigationGeneration.current += 1;
    snapshotRequests.current.beginNavigationChange(archived);
    return navigationGeneration.current;
  };
  const currentNavigationGeneration = () => navigationGeneration.current;
  const markSubscriptionError = (key: string, error: unknown) => {
    const message = error instanceof Error ? error.message : "Unable to refresh App Server state.";
    failedSubscriptionBaselines.current.set(key, message);
    setBackendConnectionState({ status: "reconnecting", message });
  };
  const markSubscriptionReady = (key: string) => {
    failedSubscriptionBaselines.current.delete(key);
    const remainingMessage = [...failedSubscriptionBaselines.current.values()].at(-1);
    if (remainingMessage) {
      setBackendConnectionState({ status: "reconnecting", message: remainingMessage });
      return;
    }
    if (routeOpenError.current) {
      setBackendConnectionState({ status: "unavailable", message: routeOpenError.current });
      return;
    }
    if (backendInitialized.current) setBackendConnectionState({ status: "ready" });
  };

  useEffect(() => {
    if (initialBootstrap.surface === "invalid") return;
    let active = true;
    const serverRequestBridge = backendConnection?.serverRequests
      ? startAppServerServerRequestBridge({
          backendConnection,
          onPermissionRequest: (requestId, message, taskId) => {
            dispatch({
              type: "appServerPermission:received",
              requestId,
              message,
              taskId,
            });
          },
          onQuestionRequest: (requestId, message, taskId) => {
            dispatch({
              type: "appServerQuestion:received",
              requestId,
              message,
              taskId,
            });
          },
          postHostMessage,
        })
      : undefined;
    const stopSubscriptions: Array<() => void> = [];
    const stopBackendStateResets = backendConnection?.stateResets?.(() => {
      backendStateGenerationRef.current += 1;
      setBackendStateGeneration(backendStateGenerationRef.current);
      setReadyTaskSubscriptionKey(undefined);
      setReadyRouteOpenKey(undefined);
      routeOpenError.current = undefined;
      setBackendConnectionState({
        status: "reconnecting",
        message: "Connection interrupted. Reconnecting automatically.",
      });
    });
    backendStateGenerationRef.current += 1;
    setBackendStateGeneration(backendStateGenerationRef.current);
    backendInitialized.current = false;
    failedSubscriptionBaselines.current.clear();
    routeOpenError.current = undefined;
    setBackendReady(false);
    setBackendConnectionState({ status: "connecting" });
    setReadyTaskSubscriptionKey(undefined);
    setReadyRouteOpenKey(undefined);
    if (initialBootstrap.surface === "navigation") {
      const archived = initialBootstrap.archived === true;
      beginNavigationChange(archived);
      dispatch({ type: "archive:set", showArchived: archived });
    }
    const startupNavigationGeneration = navigationGeneration.current;
    postControllerStarted(postHostMessage, initialBootstrap);
    const stopSession = startHostMessageSession(subscribeHostMessages, (message) => {
      if (serverRequestBridge?.handleHostMessage(message)) return;
      routeHostMessage(message, {
        bootstrap: bootstrapRef.current,
        dispatch,
        setAgents,
        setPreferences,
        snapshotRequests,
        latestOptionsRequestKey,
        latestSessionListRequestId,
        nextSessionListRequestId,
        latestNativeSessionSelection,
        createSnapshotRequestId,
        postHostMessage,
      });
    }, () => {
      if (backendConnection) {
        void backendConnection
          .initialize(initializeParamsForBootstrap(initialBootstrap))
          .then((result) => {
            if (!active) return;
            dispatch({ type: "appServer:ready" });
            const canApplyStartupNavigation = initialBootstrap.surface !== "navigation"
              || (navigationGeneration.current === startupNavigationGeneration
                && !snapshotRequests.current.currentArchived());
            const ingestion = actionsFromInitialSnapshot(result.snapshot, {
              includeTaskNavigation: canApplyStartupNavigation,
              includeActiveTask: initialBootstrap.surface === "task",
            });
            const subscriptionContext = mappingContextFromClientSnapshot(result.snapshot);
            stateSubscriptionContext.current = subscriptionContext;
            for (const action of ingestion.actions) {
              if (action.type === "settings:preferences") setPreferences(action.preferences);
              dispatch(action);
            }
            applyProtocolAgents(result.snapshot.agents, currentAgentId.current, setAgents, dispatch);
            if (backendConnection.events) {
              const subscriptionConnection = {
                events: backendConnection.events,
                request: backendConnection.request,
                stateResets: backendConnection.stateResets,
              };
              stopSubscriptions.push(startAppServerStateSubscription({
                backendConnection: subscriptionConnection,
                context: subscriptionContext,
                dispatch,
                onBaselineError: (error) => markSubscriptionError("projects", error),
                onBaselineReady: () => markSubscriptionReady("projects"),
                scope: { kind: "projects" },
              }));
              stopSubscriptions.push(startAppServerStateSubscription({
                backendConnection: subscriptionConnection,
                context: subscriptionContext,
                currentAgentId: () => currentAgentId.current,
                dispatch,
                onBaselineError: (error) => markSubscriptionError("agents", error),
                onBaselineReady: () => markSubscriptionReady("agents"),
                scope: { kind: "agents" },
                setAgents,
              }));
              stopSubscriptions.push(startAppServerStateSubscription({
                backendConnection: subscriptionConnection,
                context: subscriptionContext,
                dispatch,
                onBaselineError: (error) => markSubscriptionError("task-navigation", error),
                onBaselineReady: () => markSubscriptionReady("task-navigation"),
                scope: taskNavigationScopeForBootstrap(initialBootstrap),
              }));
            }
            if (initialBootstrap.surface === "settings") {
              if (initialBootstrap.settingsTab) {
                dispatch({ type: "settings:tab", tab: initialBootstrap.settingsTab });
              }
              dispatch({ type: "settings:start" });
              void refreshSettingsProjectionsThroughBackend({
                backendConnection: { request: backendConnection.request },
                currentAgentId: currentAgentId.current,
                dispatch,
                setAgents,
                state,
              }).catch((error) => {
                dispatch({
                  type: "settings:error",
                  message: error instanceof Error
                    ? error.message
                    : "Unable to load Agent settings from App Server",
                });
              });
            }
            requestMissingInitialTaskList(
              {
                acceptTaskList: () => navigationGeneration.current === startupNavigationGeneration
                  && snapshotRequests.current.currentArchived()
                    === (initialBootstrap.surface === "navigation" && initialBootstrap.archived === true),
                backendConnection,
                dispatch,
              },
              initialBootstrap,
              result.snapshot,
            );
            // Route opening also starts App Server recovery work, so the route effect must
            // own task/open even when initialize already supplied cached task state.
            backendInitialized.current = true;
            setBackendReady(true);
            if (failedSubscriptionBaselines.current.size === 0) {
              setBackendConnectionState({ status: "ready" });
            }
          })
          .catch((error) => {
            if (!active) return;
            backendInitialized.current = false;
            setBackendReady(false);
            const message = error instanceof Error ? error.message : "Unable to connect to App Server.";
            setBackendConnectionState({ status: "unavailable", message });
            dispatch({
              type: "appServer:error",
              message,
            });
            dispatchStartupReadError(bootstrap, dispatch);
          });
      } else {
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
      failedSubscriptionBaselines.current.clear();
      lastRequestedRouteTaskKey.current = undefined;
      routeOpenInFlight.current = undefined;
      routeOpenError.current = undefined;
      setBackendReady(false);
      setReadyTaskSubscriptionKey(undefined);
      setReadyRouteOpenKey(undefined);
      stateSubscriptionContext.current = undefined;
      stopBackendStateResets?.();
      for (const stop of stopSubscriptions) stop();
      serverRequestBridge?.dispose();
      stopSession();
      backendConnection?.close();
    };
  }, [backendConnection, initialBootstrap.surface, initialBootstrap.taskId]);

  useEffect(() => {
    if (!backendConnection?.events || !backendReady || !backendInitialized.current || !state.snapshot) return;
    const context = stateSubscriptionContext.current;
    if (!context) return;
    const taskId = state.snapshot.task.task_id;
    const subscriptionKey = `${backendStateGeneration}:${taskId}`;
    let active = true;
    const stop = startAppServerStateSubscription({
      backendConnection: {
        events: backendConnection.events,
        request: backendConnection.request,
      },
      context,
      dispatch,
      onBaselineLost: () => {
        if (!active) return;
        setBackendConnectionState({
          status: "reconnecting",
          message: "Connection interrupted. Reconnecting automatically.",
        });
        setReadyTaskSubscriptionKey((current) => (
          current === subscriptionKey ? undefined : current
        ));
      },
      onBaselineError: (error) => {
        if (active) markSubscriptionError(subscriptionKey, error);
      },
      onBaselineReady: () => {
        if (!active) return;
        setReadyTaskSubscriptionKey(subscriptionKey);
        markSubscriptionReady(subscriptionKey);
      },
      scope: { kind: "task", taskId: taskId as TaskId },
    });
    return () => {
      active = false;
      failedSubscriptionBaselines.current.delete(subscriptionKey);
      stop();
    };
  }, [backendConnection, backendReady, backendStateGeneration, state.snapshot?.task.task_id]);

  useEffect(() => {
    if (bootstrap.surface !== "task" || !bootstrap.taskId) {
      lastRequestedRouteTaskKey.current = undefined;
      routeOpenInFlight.current = undefined;
      routeOpenError.current = undefined;
      if (backendInitialized.current && failedSubscriptionBaselines.current.size === 0) {
        setBackendConnectionState({ status: "ready" });
      }
      return;
    }
    const taskId = bootstrap.taskId;
    if (!backendConnection) {
      dispatch({
        type: "taskOpen:error",
        taskId,
        message: "App Server connection unavailable.",
      });
      return;
    }
    if (!backendInitialized.current) return;
    const requestKey = `${backendStateGeneration}:${taskId}`;
    if (lastRequestedRouteTaskKey.current === requestKey) return;
    if (routeOpenInFlight.current?.taskId === taskId) return;
    lastRequestedRouteTaskKey.current = requestKey;
    const wasUnavailable = routeOpenError.current !== undefined;
    routeOpenError.current = undefined;
    if (wasUnavailable) setBackendConnectionState({ status: "connecting" });
    const requestGeneration = backendStateGeneration;
    let openAccepted = false;

    const openRequest = requestTaskOpen({
      acceptTaskOpen: (openedTaskId, requestId, intent) => {
        if (backendStateGenerationRef.current !== requestGeneration) return false;
        openAccepted = acceptSnapshotRequest(openedTaskId, requestId, intent);
        return openAccepted;
      },
      backendConnection,
      createTaskOpenRequestId: createSnapshotRequestId,
      dispatch,
    }, taskId, "open")
      .then(() => {
        if (openAccepted && backendStateGenerationRef.current === requestGeneration) {
          setReadyRouteOpenKey(requestKey);
          if (failedSubscriptionBaselines.current.size === 0) {
            setBackendConnectionState({ status: "ready" });
          }
        }
      })
      .catch((error) => {
        const message = error instanceof Error ? error.message : "Unable to open task from App Server.";
        routeOpenError.current = message;
        setBackendConnectionState({ status: "unavailable", message });
        dispatch({ type: "taskOpen:error", taskId, message });
      });
    routeOpenInFlight.current = { promise: openRequest, taskId };
    void openRequest.finally(() => {
      if (routeOpenInFlight.current?.promise === openRequest) {
        routeOpenInFlight.current = undefined;
      }
      // A reset can supersede an open already in flight. Re-run the effect after
      // settlement so the current Backend generation receives its own task/open.
      if (backendInitialized.current) {
        setRouteOpenSettlement((settlement) => settlement + 1);
      }
    });
  }, [
    backendConnection,
    backendReady,
    backendStateGeneration,
    bootstrap.surface,
    bootstrap.taskId,
    routeOpenSettlement,
  ]);

  const taskSubscriptionKey = state.snapshot
    ? `${backendStateGeneration}:${state.snapshot.task.task_id}`
    : undefined;
  const taskSubscriptionReady = !backendConnection?.events
    || taskSubscriptionKey === undefined
    || readyTaskSubscriptionKey === taskSubscriptionKey;
  const routeOpenKey = bootstrap.surface === "task" && bootstrap.taskId
    ? `${backendStateGeneration}:${bootstrap.taskId}`
    : undefined;
  const routeOpenReady = routeOpenKey === undefined || readyRouteOpenKey === routeOpenKey;
  const retryTaskOpen = useCallback(() => {
    if (bootstrap.surface !== "task" || !bootstrap.taskId || !backendInitialized.current) return;
    lastRequestedRouteTaskKey.current = undefined;
    routeOpenError.current = undefined;
    setReadyRouteOpenKey(undefined);
    setBackendConnectionState({ status: "reconnecting", message: "Retrying task open." });
    dispatch({ type: "taskOpen:start", taskId: bootstrap.taskId });
    setRouteOpenSettlement((settlement) => settlement + 1);
  }, [bootstrap.surface, bootstrap.taskId, dispatch]);

  return {
    acceptSnapshotRequest,
    backendInitialized,
    backendConnectionState,
    backendReady: backendReady && taskSubscriptionReady && routeOpenReady,
    beginNavigationChange,
    bootstrap,
    createSnapshotRequestId,
    currentNavigationGeneration,
    retryTaskOpen,
  };
}
