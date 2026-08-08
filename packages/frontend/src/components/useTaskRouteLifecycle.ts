import { useCallback, useEffect, useRef, useState, type Dispatch, type MutableRefObject, type RefObject } from "react";
import type { TaskSnapshot } from "@openaide/app-shell-contracts";
import { AppServerProtocolError, type TaskId } from "@openaide/app-server-client";
import { requestTaskOpen } from "../intents/taskReadIntents";
import {
  startAppServerStateSubscription,
  type StateSubscriptionMappingContext,
} from "../services/appServerStateSubscriptions";
import { bindAppServerReplicaEpoch, type AppAction, type SnapshotIntent } from "../state/appReducer";
import type { AsyncOperationOwner } from "../state/asyncOperationOwner";
import { sendWebviewTelemetry } from "../state/hostMessageRouter";
import type { WebviewBootstrap } from "../state/surfaceTypes";
import { postHostMessage } from "../services/hostBridge";
import type {
  AppControllerBackendConnection,
  BackendConnectionState,
} from "./appControllerBackendLifecycle";

type TaskRouteLifecycleOptions = {
  acceptSnapshotRequest(taskId: string, requestId: number | undefined, intent: SnapshotIntent): boolean;
  backendConnection?: AppControllerBackendConnection;
  backendInitialized: RefObject<boolean>;
  backendReady: boolean;
  backendStateGeneration: number;
  bootstrap: WebviewBootstrap;
  createSnapshotRequestId(taskId?: string, intent?: SnapshotIntent): number;
  dispatch: Dispatch<AppAction>;
  failedSubscriptionBaselines: MutableRefObject<Map<string, string>>;
  markSubscriptionError(key: string, error: unknown): void;
  markSubscriptionReady(key: string): void;
  operationOwner: AsyncOperationOwner;
  replicaEpochRef: RefObject<number>;
  setBackendConnectionState(state: BackendConnectionState): void;
  snapshot?: TaskSnapshot;
  stateSubscriptionContext: RefObject<StateSubscriptionMappingContext | undefined>;
};

/** Owns the subscription, open request, readiness, and retry policy for the routed Task. */
export function useTaskRouteLifecycle({
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
  snapshot,
  stateSubscriptionContext,
}: TaskRouteLifecycleOptions) {
  const [readyTaskSubscriptionKey, setReadyTaskSubscriptionKey] = useState<string | undefined>();
  const [readyRouteOpenKey, setReadyRouteOpenKey] = useState<string | undefined>();
  const [routeOpenSettlement, setRouteOpenSettlement] = useState(0);
  const lastRequestedRouteTaskKey = useRef<string | undefined>(undefined);
  const routeOpenInFlight = useRef<{
    promise: Promise<void>;
    requestKey: string;
  } | undefined>(undefined);

  const reset = useCallback(() => {
    lastRequestedRouteTaskKey.current = undefined;
    routeOpenInFlight.current = undefined;
    setReadyTaskSubscriptionKey(undefined);
    setReadyRouteOpenKey(undefined);
  }, []);

  useEffect(() => {
    if (!backendConnection || !backendReady || !backendInitialized.current || !snapshot) return;
    const context = stateSubscriptionContext.current;
    if (!context) return;
    const taskId = snapshot.task.task_id;
    const subscriptionKey = `${backendStateGeneration}:${taskId}`;
    const subscriptionStartedAt = Date.now();
    sendWebviewTelemetry(postHostMessage, "task_state_subscription_started", {
      task_id: taskId,
      generation: backendStateGeneration,
    });
    let active = true;
    const stop = startAppServerStateSubscription({
      backendConnection,
      context,
      dispatch,
      onBaselineLost: () => {
        if (!active) return;
        sendWebviewTelemetry(postHostMessage, "task_state_baseline_lost", {
          task_id: taskId,
          generation: backendStateGeneration,
        });
        setBackendConnectionState({
          status: "reconnecting",
          message: "Connection interrupted. Reconnecting automatically.",
        });
        setReadyTaskSubscriptionKey((current) => current === subscriptionKey ? undefined : current);
      },
      onBaselineError: (error) => {
        if (!active) return;
        sendWebviewTelemetry(postHostMessage, "task_state_baseline_failed", {
          task_id: taskId,
          generation: backendStateGeneration,
          error_name: error instanceof Error ? error.name : typeof error,
        });
        markSubscriptionError(subscriptionKey, error);
      },
      onBaselineReady: () => {
        if (!active) return;
        sendWebviewTelemetry(postHostMessage, "task_state_baseline_ready", {
          task_id: taskId,
          generation: backendStateGeneration,
          duration_ms: Date.now() - subscriptionStartedAt,
        });
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
  }, [backendConnection, backendReady, backendStateGeneration, snapshot?.task.task_id]);

  useEffect(() => {
    if (bootstrap.surface !== "task" || !bootstrap.taskId) {
      lastRequestedRouteTaskKey.current = undefined;
      routeOpenInFlight.current = undefined;
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
    if (routeOpenInFlight.current?.requestKey === requestKey) return;
    lastRequestedRouteTaskKey.current = requestKey;
    const openOperation = operationOwner.claim("route-task-open", requestKey);
    const requestReplicaEpoch = replicaEpochRef.current;
    const requestDispatch = bindAppServerReplicaEpoch(dispatch, requestReplicaEpoch);
    let openAccepted = false;
    const openStartedAt = Date.now();
    sendWebviewTelemetry(postHostMessage, "task_route_open_started", {
      task_id: taskId,
      generation: backendStateGeneration,
    });

    const openRequest = requestTaskOpen({
      acceptTaskOpen: (openedTaskId, requestId, intent) => {
        if (!operationOwner.owns(openOperation)) return false;
        openAccepted = acceptSnapshotRequest(openedTaskId, requestId, intent);
        return openAccepted;
      },
      backendConnection,
      createTaskOpenRequestId: createSnapshotRequestId,
      dispatch: requestDispatch,
    }, taskId, "open")
      .then(() => {
        sendWebviewTelemetry(postHostMessage, "task_route_open_completed", {
          task_id: taskId,
          generation: backendStateGeneration,
          duration_ms: Date.now() - openStartedAt,
          accepted: openAccepted,
        });
        if (openAccepted && operationOwner.owns(openOperation)) {
          setReadyRouteOpenKey(requestKey);
        }
      })
      .catch((error) => {
        if (!operationOwner.owns(openOperation)) return;
        const message = error instanceof Error ? error.message : "Unable to open task from App Server.";
        const taskNotFound = error instanceof AppServerProtocolError
          && error.protocolError.code === "notFound";
        sendWebviewTelemetry(postHostMessage, "task_route_open_failed", {
          task_id: taskId,
          generation: backendStateGeneration,
          duration_ms: Date.now() - openStartedAt,
          error_name: error instanceof Error ? error.name : typeof error,
          error_code: error instanceof AppServerProtocolError
            ? error.protocolError.code
            : undefined,
        });
        if (taskNotFound) {
          requestDispatch({ type: "taskOpen:error", taskId, kind: "notFound", message });
          return;
        }
        requestDispatch({ type: "taskOpen:error", taskId, message });
      });
    routeOpenInFlight.current = { promise: openRequest, requestKey };
    void openRequest.finally(() => {
      if (routeOpenInFlight.current?.promise === openRequest) routeOpenInFlight.current = undefined;
      // A reset can supersede an open already in flight. Re-run after settlement
      // so the current Backend generation receives its own task/open.
      if (backendInitialized.current) setRouteOpenSettlement((settlement) => settlement + 1);
    });
  }, [
    backendConnection,
    backendReady,
    backendStateGeneration,
    bootstrap.surface,
    bootstrap.taskId,
    routeOpenSettlement,
  ]);

  const taskSubscriptionKey = snapshot
    ? `${backendStateGeneration}:${snapshot.task.task_id}`
    : undefined;
  const taskSubscriptionReady = !backendConnection
    || taskSubscriptionKey === undefined
    || readyTaskSubscriptionKey === taskSubscriptionKey;
  const routeOpenKey = bootstrap.surface === "task" && bootstrap.taskId
    ? `${backendStateGeneration}:${bootstrap.taskId}`
    : undefined;
  const routeOpenReady = routeOpenKey === undefined || readyRouteOpenKey === routeOpenKey;
  const retryTaskOpen = useCallback(() => {
    if (bootstrap.surface !== "task" || !bootstrap.taskId || !backendInitialized.current) return;
    lastRequestedRouteTaskKey.current = undefined;
    setReadyRouteOpenKey(undefined);
    dispatch({ type: "taskOpen:start", taskId: bootstrap.taskId });
    setRouteOpenSettlement((settlement) => settlement + 1);
  }, [bootstrap.surface, bootstrap.taskId, dispatch]);

  return {
    ready: taskSubscriptionReady && routeOpenReady,
    reset,
    retryTaskOpen,
  };
}
