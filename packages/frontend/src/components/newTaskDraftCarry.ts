import type { AttachmentHandleId, BackendConnection } from "@openaide/app-server-client";
import {
  transferAttachmentHandles,
  type ComposerAttachmentResourceOwner,
} from "../services/attachmentResources";
import { recordFrontendLifecycleEvent } from "../services/frontendLifecycleTelemetry";
import type { AppAction } from "../state/appReducer";
import type { AppState } from "../state/store";

export const ATTACHMENT_LOST_ON_TASK_CHANGE =
  "This attachment could not follow the Agent or workspace change. Attach it again.";

type PendingCarry = { fromTaskId: string; handleIds: AttachmentHandleId[] };

/** App Server handle ids the unsent New Task draft shows for one Prepared Task. */
export function draftAttachmentHandleIds(state: AppState, taskId: string): AttachmentHandleId[] {
  const handleIds = new Set<AttachmentHandleId>();
  for (const attachment of [...state.newTask.context, ...(state.taskInputs[taskId]?.context ?? [])]) {
    if (attachment.app_server_handle_id) handleIds.add(attachment.app_server_handle_id);
  }
  return [...handleIds];
}

/**
 * Carries an unsent draft's App Server handles across Prepared Task replacement.
 *
 * Changing Agent, Project, or worktree discards the current Prepared Task before its
 * successor is acquired. The draft keeps its rows, so its handles are held (never
 * released) from that discard until `attachment/transfer` moves them to whichever Task
 * is acquired next. A carry that outlives a superseded or failed preparation waits for the
 * next successor rather than being dropped.
 */
export class NewTaskDraftCarry {
  private pending: PendingCarry[] = [];

  hold(
    attachmentResources: ComposerAttachmentResourceOwner | undefined,
    fromTaskId: string,
    handleIds: readonly AttachmentHandleId[],
  ) {
    if (!handleIds.length) return;
    attachmentResources?.holdForTransfer(fromTaskId, handleIds);
    this.pending.push({ fromTaskId, handleIds: [...handleIds] });
  }

  /**
   * Moves every held handle to `toTaskId`, then marks only the rows App Server could not
   * move. Resolves after the moves, so a Send awaiting this preparation sees valid handles.
   */
  async transferTo({
    attachmentResources,
    backendConnection,
    dispatch,
    toTaskId,
  }: {
    attachmentResources?: ComposerAttachmentResourceOwner;
    backendConnection: Pick<BackendConnection, "request">;
    dispatch: (action: AppAction) => void;
    toTaskId: string;
  }) {
    const carries = this.pending;
    this.pending = [];
    const lost: AttachmentHandleId[] = [];
    for (const carry of carries) {
      if (carry.fromTaskId === toTaskId) {
        // The pool handed the same Task back; its handles never left.
        attachmentResources?.completeTransfer(carry.fromTaskId, toTaskId, carry.handleIds, carry.handleIds);
        continue;
      }
      const outcome = await transferAttachmentHandles(
        backendConnection,
        carry.fromTaskId,
        toTaskId,
        carry.handleIds,
        (event, fields) => recordFrontendLifecycleEvent(backendConnection, event, fields),
      );
      attachmentResources?.completeTransfer(carry.fromTaskId, toTaskId, carry.handleIds, outcome.transferred);
      lost.push(...outcome.lost);
    }
    if (lost.length) {
      dispatch({ type: "newTask:attachments:lost", handleIds: lost, message: ATTACHMENT_LOST_ON_TASK_CHANGE });
    }
  }

  pendingCount() {
    return this.pending.reduce((count, carry) => count + carry.handleIds.length, 0);
  }

  /** Forgets carries whose Task and handle ids no longer identify the current App Server state. */
  clear() {
    this.pending = [];
  }
}
