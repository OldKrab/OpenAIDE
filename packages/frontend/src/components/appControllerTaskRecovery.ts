import { useEffect, useRef, type Dispatch, type RefObject } from "react";
import type { BackendConnection } from "@openaide/app-server-client";
import type { TaskSnapshot } from "@openaide/app-shell-contracts";
import { sendTaskPromptIntent } from "../intents/taskMutationIntents";
import type { AppAction, SnapshotIntent } from "../state/appReducer";
import type { PostHostMessage } from "../state/postHostMessage";
import type { AppState, TaskComposerInput } from "../state/store";

export function usePreparedTaskSendRetry({
  backendConnectionRef,
  backendInitialized,
  createSnapshotRequestId,
  dispatch,
  postHostMessage,
  state,
}: {
  backendConnectionRef?: Pick<BackendConnection, "request">;
  backendInitialized: RefObject<boolean>;
  createSnapshotRequestId: (taskId?: string, intent?: SnapshotIntent) => number;
  dispatch: Dispatch<AppAction>;
  postHostMessage: PostHostMessage;
  state: AppState;
}) {
  const retriedSend = useRef<string | undefined>(undefined);
  const snapshot = state.snapshot;
  const taskInput = snapshot ? state.taskInputs[snapshot.task.task_id] : undefined;

  useEffect(() => {
    if (!backendInitialized.current || !backendConnectionRef?.request || !snapshot || !taskInput) return;
    if (!preparedPromptNeedsRetry(snapshot, taskInput)) return;
    const retryKey = `${snapshot.task.task_id}:${snapshot.revision}:${taskInput.prompt}`;
    if (retriedSend.current === retryKey) return;
    retriedSend.current = retryKey;
    sendTaskPromptIntent(
      { backendConnection: backendConnectionRef, createSnapshotRequestId, dispatch, postHostMessage },
      snapshot,
      taskInput,
    );
  }, [
    backendConnectionRef,
    backendInitialized,
    createSnapshotRequestId,
    dispatch,
    postHostMessage,
    snapshot,
    taskInput,
  ]);
}

export function preparedPromptNeedsRetry(snapshot: TaskSnapshot, input: TaskComposerInput) {
  return (
    snapshot.task.status === "inactive"
    && !snapshot.chat.items.some((item) => item.message_id === "app-server-preparation")
    && input.pending === undefined
    && input.prompt.trim().length > 0
    && input.error === "Task Agent preparation is still running"
  );
}
