import {
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type RefObject,
  type SetStateAction,
} from "react";
import type { AppPreferencesRecord } from "@openaide/app-shell-contracts";
import type { BackendConnection, TaskId } from "@openaide/app-server-client";
import { requestMissingInitialTaskRead, requestTaskOpen } from "../intents/taskReadIntents";
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
  const backendInitialized = useRef(false);
  const lastRequestedRouteTaskId = useRef<string | undefined>(undefined);
  const navigationGeneration = useRef(0);
  const stateSubscriptionContext = useRef<StateSubscriptionMappingContext | undefined>(undefined);
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
    backendInitialized.current = false;
    setBackendReady(false);
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
                scope: { kind: "projects" },
              }));
              stopSubscriptions.push(startAppServerStateSubscription({
                backendConnection: subscriptionConnection,
                context: subscriptionContext,
                currentAgentId: () => currentAgentId.current,
                dispatch,
                scope: { kind: "agents" },
                setAgents,
              }));
              stopSubscriptions.push(startAppServerStateSubscription({
                backendConnection: subscriptionConnection,
                context: subscriptionContext,
                dispatch,
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
            requestMissingInitialTaskRead(
              {
                acceptTaskOpen: acceptSnapshotRequest,
                acceptTaskList: () => navigationGeneration.current === startupNavigationGeneration
                  && snapshotRequests.current.currentArchived()
                    === (initialBootstrap.surface === "navigation" && initialBootstrap.archived === true),
                backendConnection,
                createTaskOpenRequestId: createSnapshotRequestId,
                dispatch,
              },
              initialBootstrap,
              result.snapshot,
            );
            if (initialBootstrap.surface === "task" && initialBootstrap.taskId) {
              lastRequestedRouteTaskId.current = initialBootstrap.taskId;
            }
            backendInitialized.current = true;
            setBackendReady(true);
          })
          .catch((error) => {
            if (!active) return;
            backendInitialized.current = false;
            setBackendReady(false);
            dispatch({
              type: "appServer:error",
              message: error instanceof Error ? error.message : "Unable to connect to App Server.",
            });
            dispatchStartupReadError(bootstrap, dispatch);
          });
      } else {
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
      lastRequestedRouteTaskId.current = undefined;
      setBackendReady(false);
      stateSubscriptionContext.current = undefined;
      for (const stop of stopSubscriptions) stop();
      serverRequestBridge?.dispose();
      stopSession();
      backendConnection?.close();
    };
  }, [backendConnection, initialBootstrap.surface, initialBootstrap.taskId]);

  useEffect(() => {
    if (!backendConnection?.events || !backendInitialized.current || !state.snapshot) return;
    const context = stateSubscriptionContext.current;
    if (!context) return;
    return startAppServerStateSubscription({
      backendConnection: {
        events: backendConnection.events,
        request: backendConnection.request,
        stateResets: backendConnection.stateResets,
      },
      context,
      dispatch,
      scope: { kind: "task", taskId: state.snapshot.task.task_id as TaskId },
    });
  }, [backendConnection, state.snapshot?.task.task_id]);

  useEffect(() => {
    if (bootstrap.surface !== "task" || !bootstrap.taskId) {
      lastRequestedRouteTaskId.current = undefined;
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
    if (lastRequestedRouteTaskId.current === taskId) return;
    lastRequestedRouteTaskId.current = taskId;
    void requestTaskOpen({
      acceptTaskOpen: acceptSnapshotRequest,
      backendConnection,
      createTaskOpenRequestId: createSnapshotRequestId,
      dispatch,
    }, taskId, "open").catch(() => dispatch({
      type: "taskOpen:error",
      taskId,
      message: "Unable to open task from App Server",
    }));
  }, [backendConnection, backendReady, bootstrap.surface, bootstrap.taskId]);

  return {
    acceptSnapshotRequest,
    backendInitialized,
    backendReady,
    beginNavigationChange,
    bootstrap,
    createSnapshotRequestId,
    currentNavigationGeneration,
  };
}
