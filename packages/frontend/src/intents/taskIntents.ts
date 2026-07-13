import { PERMISSION_REQUEST, QUESTION_REQUEST, type BackendConnection, type RequestId } from "@openaide/app-server-client";
import type { ElicitationResponse } from "@openaide/app-shell-contracts";
import type { AppAction } from "../state/appReducer";
import type { AppState } from "../state/store";

export type TaskIntentDependencies = {
  backendConnection?: Pick<BackendConnection, "respond">;
  dispatch: (action: AppAction) => void;
  state: AppState;
};

export function respondToPermissionIntent(
  dependencies: TaskIntentDependencies,
  requestId: string,
  optionId: string,
) {
  const { backendConnection, dispatch, state } = dependencies;
  if (!state.snapshot) return;

  dispatch({ type: "permission:responding", requestId });
  if (!backendConnection) {
    dispatchPermissionError(dispatch, requestId, new Error("App Server connection unavailable"));
    return;
  }
  let result: Promise<void> | void;
  try {
    result = backendConnection.respond<typeof PERMISSION_REQUEST>(requestId as RequestId, { optionId });
  } catch (error) {
    dispatchPermissionError(dispatch, requestId, error);
    return;
  }
  if (isPromiseLike(result)) {
    void result.catch((error) => dispatchPermissionError(dispatch, requestId, error));
    return;
  }
}

export function respondToQuestionIntent(
  dependencies: TaskIntentDependencies,
  requestId: string,
  response: ElicitationResponse,
) {
  const { backendConnection, dispatch } = dependencies;
  dispatch({ type: "question:responding", requestId });
  if (!backendConnection) {
    dispatchQuestionError(dispatch, requestId, new Error("App Server connection unavailable"));
    return;
  }
  let result: Promise<void> | void;
  try {
    result = backendConnection.respond<typeof QUESTION_REQUEST>(requestId as RequestId, response);
  } catch (error) {
    dispatchQuestionError(dispatch, requestId, error);
    return;
  }
  if (isPromiseLike(result)) {
    void result.catch((error) => dispatchQuestionError(dispatch, requestId, error));
    return;
  }
}

function isPromiseLike(value: void | Promise<void>): value is Promise<void> {
  return Boolean(value && typeof value === "object" && "catch" in value);
}

function dispatchPermissionError(
  dispatch: (action: AppAction) => void,
  requestId: string,
  error: unknown,
) {
  dispatch({
    type: "permission:error",
    requestId,
    message: error instanceof Error ? error.message : "Permission response failed",
  });
}

function dispatchQuestionError(
  dispatch: (action: AppAction) => void,
  requestId: string,
  error: unknown,
) {
  dispatch({
    type: "question:error",
    requestId,
    message: error instanceof Error ? error.message : "Question response failed",
  });
}
