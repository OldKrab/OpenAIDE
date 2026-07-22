import { useEffect, useRef, type Dispatch } from "react";
import {
  AppServerProtocolError,
  TASK_ADOPT_NATIVE_SESSION,
  type AgentId,
  type BackendConnection,
} from "@openaide/app-server-client";
import type { AppAction } from "../state/appReducer";
import type { AsyncOperationOwner } from "../state/asyncOperationOwner";
import { taskNavigationTarget } from "../state/asyncOperationOwner";
import type { AppState } from "../state/store";
import type { WebviewBootstrap } from "../state/surfaceTypes";
import { mapProtocolTaskSnapshot } from "../state/appServerProtocolMapping";
import { openTaskSurface, postHostMessage } from "../services/hostBridge";
import { sendWebviewTelemetry } from "../state/hostMessageRouter";
import type { ComposerAttachmentResourceOwner } from "../services/attachmentResources";
import { discardPreparedNewTask } from "./navigationCallbacks";
import type { NewTaskController } from "./newTaskController";

/** Owns adoption only while the pre-Task Native Session route remains current. */
export function useNativeSessionRouteLifecycle({
  asyncOperations,
  attachmentResources,
  backendConnection,
  backendReady,
  bootstrap,
  dispatch,
  newTaskController,
  replicaEpoch,
  state,
}: {
  asyncOperations: AsyncOperationOwner;
  attachmentResources: ComposerAttachmentResourceOwner;
  backendConnection?: Pick<BackendConnection, "request">;
  backendReady: boolean;
  bootstrap: WebviewBootstrap;
  dispatch: Dispatch<AppAction>;
  newTaskController: NewTaskController;
  replicaEpoch: number;
  state: AppState;
}) {
  const startedRouteKey = useRef<string | undefined>(undefined);
  const routedAgentId = bootstrap.surface === "nativeSession" ? bootstrap.agentId : undefined;
  const routedNativeSessionId = bootstrap.surface === "nativeSession" ? bootstrap.nativeSessionId : undefined;

  useEffect(() => {
    if (bootstrap.surface !== "nativeSession") {
      startedRouteKey.current = undefined;
      return;
    }
    const agentId = routedAgentId;
    const nativeSessionId = routedNativeSessionId;
    if (!agentId || !nativeSessionId || !backendReady || !backendConnection) return;
    const routeKey = `${replicaEpoch}\u0000${agentId}\u0000${nativeSessionId}`;
    if (startedRouteKey.current === routeKey) return;
    startedRouteKey.current = routeKey;

    void adoptRoutedNativeSession({
      agentId,
      asyncOperations,
      attachmentResources,
      backendConnection,
      dispatch,
      nativeSessionId,
      newTaskController,
      routeKey,
      state,
    });
  }, [
    asyncOperations,
    attachmentResources,
    backendConnection,
    backendReady,
    bootstrap.surface,
    dispatch,
    newTaskController,
    replicaEpoch,
    routedAgentId,
    routedNativeSessionId,
    state,
  ]);
}

export async function adoptRoutedNativeSession({
  agentId,
  asyncOperations,
  attachmentResources,
  backendConnection,
  dispatch,
  nativeSessionId,
  newTaskController,
  routeKey,
  state,
}: {
  agentId: string;
  asyncOperations: AsyncOperationOwner;
  attachmentResources: ComposerAttachmentResourceOwner;
  backendConnection: Pick<BackendConnection, "request">;
  dispatch: Dispatch<AppAction>;
  nativeSessionId: string;
  newTaskController: NewTaskController;
  routeKey: string;
  state: AppState;
}) {
  const operation = asyncOperations.claim("native-session-adoption", routeKey);
  sendWebviewTelemetry(postHostMessage, "native_session_route_adoption_started", {
    agent_id: agentId,
    native_session_id: nativeSessionId,
  });
  dispatch({ type: "newTask:nativeSessions:adopt", sessionId: nativeSessionId });
  try {
    await discardPreparedNewTask({
      attachmentResources,
      backendConnection,
      dispatch,
      newTaskController,
      state,
    });
    const result = await backendConnection.request(TASK_ADOPT_NATIVE_SESSION, {
      agentId: agentId as AgentId,
      nativeSessionId,
    });
    if (!asyncOperations.owns(operation)) return;
    const snapshot = mapProtocolTaskSnapshot(result.task).snapshot;
    dispatch({ type: "snapshot", snapshot, intent: "open" });
    dispatch({ type: "newTask:nativeSessions:remove", sessionId: nativeSessionId });
    asyncOperations.expectNavigation(taskNavigationTarget(snapshot.task.task_id));
    openTaskSurface(snapshot.task.task_id, snapshot.task.title);
  } catch (error) {
    const ownsOperation = asyncOperations.owns(operation);
    sendWebviewTelemetry(postHostMessage, "native_session_route_adoption_failed", {
      agent_id: agentId,
      native_session_id: nativeSessionId,
      error_name: error instanceof Error ? error.name : typeof error,
      error_code: error instanceof AppServerProtocolError ? error.protocolError.code : undefined,
      owns_operation: ownsOperation,
    });
    if (!ownsOperation) return;
    const noLongerExists = error instanceof AppServerProtocolError
      && error.protocolError.code === "notFound";
    dispatch({
      type: "newTask:nativeSessions:error",
      sessionId: nativeSessionId,
      message: noLongerExists
        ? "This session no longer exists."
        : error instanceof Error ? error.message : "Unable to open task.",
    });
    if (noLongerExists) {
      dispatch({ type: "newTask:nativeSessions:remove", sessionId: nativeSessionId });
    }
  }
}
