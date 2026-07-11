import type { Dispatch, MutableRefObject } from "react";
import {
  AGENT_LIST_SESSIONS,
  type AgentId,
  type BackendConnection,
  type ProjectId,
} from "@openaide/app-server-client";
import type { AppAction } from "../state/appReducer";

export function requestControllerNativeSessions({
  agentId,
  append = false,
  cursor,
  backendConnection,
  dispatch,
  latestSessionListRequestId,
  nextSessionListRequestId,
  projectId,
}: {
  agentId: string;
  append?: boolean;
  cursor?: string;
  backendConnection?: Pick<BackendConnection, "request">;
  dispatch: Dispatch<AppAction>;
  latestSessionListRequestId: MutableRefObject<number | undefined>;
  nextSessionListRequestId: MutableRefObject<number>;
  projectId?: string;
}) {
  const requestId = nextSessionListRequestId.current + 1;
  nextSessionListRequestId.current = requestId;
  latestSessionListRequestId.current = requestId;
  dispatch({ type: "newTask:nativeSessions:start", append });
  if (backendConnection) {
    if (!projectId) {
      dispatch({ type: "newTask:nativeSessions:error", message: "Project is not ready yet." });
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
    }).catch(() => {
      if (latestSessionListRequestId.current !== requestId) return;
      dispatch({ type: "newTask:nativeSessions:error", message: "Unable to load tasks." });
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
}: {
  backendConnection?: Pick<BackendConnection, "request">;
  dispatch: Dispatch<AppAction>;
  getAgentId: () => string;
  getProjectId: () => string | undefined;
  latestSessionListRequestId: MutableRefObject<number | undefined>;
  nextSessionListRequestId: MutableRefObject<number>;
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
      projectId: getProjectId(),
    });
  };
}
