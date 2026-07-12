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
  existingSessionIds = [],
  latestSessionListRequestId,
  minimumSessionCount = 0,
  nextSessionListRequestId,
  projectId,
  onFailure,
}: {
  agentId: string;
  append?: boolean;
  cursor?: string;
  backendConnection?: Pick<BackendConnection, "request">;
  dispatch: Dispatch<AppAction>;
  existingSessionIds?: Iterable<string>;
  latestSessionListRequestId: MutableRefObject<number | undefined>;
  minimumSessionCount?: number;
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
      dispatch({ type: "newTask:nativeSessions:listError", message: "Workspace is not ready yet." });
      return;
    }
    const loadPages = async () => {
      let nextCursor = cursor;
      let resultAgentId = agentId;
      const sessions = new Map<string, AgentListedSessionResult>();
      const existing = append ? new Set(existingSessionIds) : new Set<string>();
      const requestedCursors = new Set<string>();
      do {
        if (nextCursor) {
          if (requestedCursors.has(nextCursor)) {
            // A cursor is an opaque continuation token, but it must still make
            // progress. Treat a repeated token as exhaustion instead of looping.
            nextCursor = undefined;
            break;
          }
          requestedCursors.add(nextCursor);
        }
        const result = await backendConnection.request(AGENT_LIST_SESSIONS, {
          agentId: agentId as AgentId,
          projectId: projectId as ProjectId,
          cursor: nextCursor ?? null,
        });
        if (latestSessionListRequestId.current !== requestId) return;
        resultAgentId = result.agentId;
        for (const session of result.sessions) {
          if (existing.has(session.sessionId) || sessions.has(session.sessionId)) continue;
          sessions.set(session.sessionId, {
            cwd: result.projectLabel,
            session_id: session.sessionId,
            title: session.title ?? undefined,
            last_activity: session.lastActivity ?? session.updatedAt ?? undefined,
            updated_at: session.updatedAt ?? undefined,
          });
        }
        nextCursor = result.nextCursor ?? undefined;
      } while (nextCursor && sessions.size < minimumSessionCount);

      if (latestSessionListRequestId.current !== requestId) return;
      dispatch({
        type: "newTask:nativeSessions:result",
        result: {
          agent_id: resultAgentId,
          next_cursor: nextCursor,
          sessions: [...sessions.values()],
        },
        append,
      });
    };
    void loadPages().catch((error: unknown) => {
      if (latestSessionListRequestId.current !== requestId) return;
      onFailure?.(nativeSessionLoadFailure(error, { agentId, projectId, requestId }));
      dispatch({ type: "newTask:nativeSessions:listError", message: "Unable to load Agent session history." });
    });
    return;
  }
  dispatch({ type: "newTask:nativeSessions:listError", message: "App Server connection unavailable." });
}

export function createRequestControllerNativeSessions({
  backendConnection,
  dispatch,
  getAgentId,
  getExistingSessionIds,
  getProjectId,
  latestSessionListRequestId,
  nextSessionListRequestId,
  onFailure,
}: {
  backendConnection?: Pick<BackendConnection, "request">;
  dispatch: Dispatch<AppAction>;
  getAgentId: () => string;
  getExistingSessionIds?: () => Iterable<string>;
  getProjectId: () => string | undefined;
  latestSessionListRequestId: MutableRefObject<number | undefined>;
  nextSessionListRequestId: MutableRefObject<number>;
  onFailure?: (failure: NativeSessionLoadFailure) => void;
}) {
  return (cursor?: string, append = false, minimumSessionCount = 0) => {
    requestControllerNativeSessions({
      agentId: getAgentId(),
      append,
      backendConnection,
      cursor,
      dispatch,
      existingSessionIds: append ? getExistingSessionIds?.() : undefined,
      latestSessionListRequestId,
      minimumSessionCount,
      nextSessionListRequestId,
      onFailure,
      projectId: getProjectId(),
    });
  };
}

type AgentListedSessionResult = {
  cwd: string;
  session_id: string;
  title?: string;
  last_activity?: string;
  updated_at?: string;
};

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
