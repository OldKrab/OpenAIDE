import { useEffect, useRef, type Dispatch, type RefObject } from "react";
import type { BackendConnection } from "@openaide/app-server-client";
import type { TaskSnapshot } from "@openaide/app-shell-contracts";
import { requestTaskOpen } from "../intents/taskReadIntents";
import { sendTaskPromptIntent } from "../intents/taskMutationIntents";
import type { AppAction, SnapshotIntent } from "../state/appReducer";
import type { PostHostMessage } from "../state/postHostMessage";
import type { AppState, TaskComposerInput } from "../state/store";

const ACTIVE_TASK_POLL_MS = 1_000;

export function useActiveTaskPolling({
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
    if (!backendInitialized.current || !backendConnectionRef?.request || !snapshot) return;
    if (!activeTaskNeedsPolling(snapshot)) return;

    const taskId = snapshot.task.task_id;
    const poll = () => {
      void requestTaskOpen({ backendConnection: backendConnectionRef, dispatch }, taskId, "refresh").catch(() => {
        // Existing task-open surfaces own recoverable errors; polling should not replace visible task state.
      });
    };
    const timer = globalThis.setInterval(poll, ACTIVE_TASK_POLL_MS);
    return () => globalThis.clearInterval(timer);
  }, [
    backendConnectionRef,
    backendInitialized,
    dispatch,
    snapshot?.revision,
    snapshot?.task.status,
    snapshot?.task.task_id,
  ]);

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

export function activeTaskNeedsPolling(snapshot: TaskSnapshot) {
  return (
    snapshot.task.status === "active"
    || snapshot.task.status === "blocked"
    || snapshot.chat.items.some((item) => item.message_id === "app-server-preparation")
  );
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
