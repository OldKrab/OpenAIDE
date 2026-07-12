import type { Dispatch, MutableRefObject } from "react";
import {
  AGENT_LIST_SESSIONS,
  AppServerProtocolError,
  type AgentId,
  type BackendConnection,
  type ProjectId,
} from "@openaide/app-server-client";
import type { AppAction } from "../state/appReducer";

export type NativeSessionLoadFailure = {
  agentId: string;
  errorCode?: string;
  errorMessage: string;
  errorName: string;
  projectId: string;
  request: typeof AGENT_LIST_SESSIONS;
  requestId: number;
};

export function requestControllerNativeSessions({
  agentId,
  append = false,
  cursor,
  backendConnection,
  dispatch,
  latestSessionListRequestId,
  nextSessionListRequestId,
  projectId,
  onFailure,
}: {
  agentId: string;
  append?: boolean;
  cursor?: string;
  backendConnection?: Pick<BackendConnection, "request">;
  dispatch: Dispatch<AppAction>;
  latestSessionListRequestId: MutableRefObject<number | undefined>;
  nextSessionListRequestId: MutableRefObject<number>;
  projectId?: string;
  onFailure?: (failure: NativeSessionLoadFailure) => void;
}) {
  const requestId = nextSessionListRequestId.current + 1;
  nextSessionListRequestId.current = requestId;
  latestSessionListRequestId.current = requestId;
  dispatch({ type: "newTask:nativeSessions:start", append });
  if (backendConnection) {
    if (!projectId) {
      dispatch({ type: "newTask:nativeSessions:error", message: "Workspace is not ready yet." });
      return;
    }
    void backendConnection.request(AGENT_LIST_SESSIONS, {
      agentId: agentId as AgentId,
      projectId: projectId as ProjectId,
      cursor: cursor ?? null,
    }).then((result) => {
      if (latestSessionListRequestId.current !== requestId) return;
      dispatch({
        type: "newTask:nativeSessions:result",
        result: {
          agent_id: result.agentId,
          next_cursor: result.nextCursor ?? undefined,
          sessions: result.sessions.map((session) => ({
            cwd: result.projectLabel,
            session_id: session.sessionId,
            title: session.title ?? undefined,
            last_activity: session.lastActivity ?? session.updatedAt ?? undefined,
            updated_at: session.updatedAt ?? undefined,
          })),
        },
        append,
      });
    }).catch((error: unknown) => {
      if (latestSessionListRequestId.current !== requestId) return;
      onFailure?.(nativeSessionLoadFailure(error, { agentId, projectId, requestId }));
      dispatch({ type: "newTask:nativeSessions:error", message: "Unable to load Agent session history." });
    });
    return;
  }
  dispatch({ type: "newTask:nativeSessions:error", message: "App Server connection unavailable." });
}

export function createRequestControllerNativeSessions({
  backendConnection,
  dispatch,
  getAgentId,
  getProjectId,
  latestSessionListRequestId,
  nextSessionListRequestId,
  onFailure,
}: {
  backendConnection?: Pick<BackendConnection, "request">;
  dispatch: Dispatch<AppAction>;
  getAgentId: () => string;
  getProjectId: () => string | undefined;
  latestSessionListRequestId: MutableRefObject<number | undefined>;
  nextSessionListRequestId: MutableRefObject<number>;
  onFailure?: (failure: NativeSessionLoadFailure) => void;
}) {
  return (cursor?: string, append = false) => {
    requestControllerNativeSessions({
      agentId: getAgentId(),
      append,
      backendConnection,
      cursor,
      dispatch,
      latestSessionListRequestId,
      nextSessionListRequestId,
      onFailure,
      projectId: getProjectId(),
    });
  };
}

function nativeSessionLoadFailure(
  error: unknown,
  context: { agentId: string; projectId: string; requestId: number },
): NativeSessionLoadFailure {
  const normalized = error instanceof Error ? error : new Error(String(error));
  return {
    ...context,
    errorCode: error instanceof AppServerProtocolError ? error.protocolError.code : undefined,
    errorMessage: normalized.message,
    errorName: normalized.name,
    request: AGENT_LIST_SESSIONS,
  };
}
