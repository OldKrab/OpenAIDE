import { useEffect, useRef } from "react";
import { TASK_MARK_READ, type BackendConnection, type TaskId } from "@openaide/app-server-client";
import type { Dispatch } from "react";
import type { AppAction } from "../state/appReducer";
import { mapProtocolTaskSnapshot } from "../state/appServerProtocolMapping";

type TaskAttentionReadReceiptOptions = {
  backendConnection?: Pick<BackendConnection, "request">;
  dispatch: Dispatch<AppAction>;
  revision?: number;
  taskId?: string;
  unread: boolean;
};

/** Acknowledges output only after post-completion user activity, never from passive visibility. */
export function useTaskAttentionReadReceipt({
  backendConnection,
  dispatch,
  revision,
  taskId,
  unread,
}: TaskAttentionReadReceiptOptions) {
  const pendingReceipt = useRef<string | undefined>(undefined);

  useEffect(() => {
    if (!backendConnection?.request || !taskId || !unread || typeof window === "undefined") return;
    const receiptKey = `${taskId}:${revision ?? "unknown"}`;
    const ownerDocument = typeof document === "undefined" ? undefined : document;
    let cancelled = false;

    const acknowledge = () => {
      if (ownerDocument?.visibilityState === "hidden") return;
      if (pendingReceipt.current === receiptKey) return;
      pendingReceipt.current = receiptKey;
      void backendConnection.request(TASK_MARK_READ, { taskId: taskId as TaskId }).then((result) => {
        if (cancelled) return;
        dispatch({
          type: "snapshot",
          snapshot: mapProtocolTaskSnapshot(result.task).snapshot,
          intent: "refresh",
        });
      }).catch(() => {
        // Keep unread authoritative and let the next attention event retry.
      }).finally(() => {
        if (pendingReceipt.current === receiptKey) pendingReceipt.current = undefined;
      });
    };

    window.addEventListener("focus", acknowledge);
    window.addEventListener("pointerdown", acknowledge, true);
    window.addEventListener("keydown", acknowledge, true);
    ownerDocument?.addEventListener("visibilitychange", acknowledge);
    return () => {
      cancelled = true;
      window.removeEventListener("focus", acknowledge);
      window.removeEventListener("pointerdown", acknowledge, true);
      window.removeEventListener("keydown", acknowledge, true);
      ownerDocument?.removeEventListener("visibilitychange", acknowledge);
    };
  }, [backendConnection, dispatch, revision, taskId, unread]);
}
