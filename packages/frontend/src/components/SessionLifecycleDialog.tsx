import { useRef, useState } from "react";
import type { NativeSessionDeleteResult, NativeSessionDeleteTarget } from "@openaide/app-server-client";
import type { DeleteSessionAction } from "../intents/sessionDeletionIntent";
import { PopupDialog } from "./Popup";

type Preview = Extract<NativeSessionDeleteResult, { kind: "confirmationRequired" }>;
type Stage = "delete" | "active";

/** Only dialog progress is local. Activity and queue counts come from the App Server. */
export function useSessionLifecycleDialog({
  title, target, onArchive, onDelete,
}: {
  title: string;
  target: NativeSessionDeleteTarget;
  onArchive: () => void;
  onDelete?: DeleteSessionAction;
}) {
  const [stage, setStage] = useState<Stage>();
  const [preview, setPreview] = useState<Preview>();
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string>();
  const busy = useRef(false);
  const readPreview = async () => {
    if (!onDelete || busy.current) return;
    busy.current = true;
    setPending(true);
    setError(undefined);
    try {
      const result = await onDelete({ target });
      if (result.kind === "confirmationRequired") setPreview(result);
    } catch (error) {
      setError(error instanceof Error ? error.message : "Unable to check this session. Try again.");
    } finally {
      busy.current = false;
      setPending(false);
    }
  };
  const apply = async () => {
    if (busy.current) return;
    if (!preview) { await readPreview(); return; }
    if (stage === "delete" && preview.active) { setStage("active"); return; }
    if (!onDelete) return;
    busy.current = true;
    setPending(true);
    setError(undefined);
    try {
      const result = await onDelete({
        target,
        confirmation: { active: stage === "active", queuedMessageCount: preview.queuedMessageCount },
      });
      if (result.kind === "deleted") setStage(undefined);
      else {
        setPreview(result);
        setStage("delete");
      }
    } catch (error) {
      setError(error instanceof Error ? error.message : "Deletion could not be confirmed. Your local history is retained. Retry Delete to check again.");
    } finally {
      busy.current = false;
      setPending(false);
    }
  };
  const heading = stage === "active" ? "Delete while work is active?" : "Delete session?";
  return {
    archive: onArchive,
    delete: () => { setPreview(undefined); setStage("delete"); void readPreview(); },
    dialog: <PopupDialog className="settings-reset-dialog session-lifecycle-dialog" label={heading} open={stage !== undefined}
      onOpenChange={(open) => { if (!open && !busy.current) setStage(undefined); }}>
      <h2>{heading}</h2>
      <p className="session-lifecycle-title">{preview?.title ?? title}</p>
      {stage === "active" ? <p>This session has active work or a pending permission request. Deleting it may interrupt that work. If the Agent refuses, the task stays available.</p>
        : <p>Ask the Agent to remove this session from its normal history. After confirmation, OpenAIDE removes its saved Chat, attachments, tool artifacts, queued messages, and Composer History. This does not promise permanent erasure. Project files and worktrees are kept.</p>}
      {preview ? <p>{preview.queuedMessageCount === 0 ? "No queued messages." : `${preview.queuedMessageCount} queued ${preview.queuedMessageCount === 1 ? "message will" : "messages will"} be discarded.`}</p> : null}
      {pending ? <p role="status">{preview ? "Waiting for the Agent…" : "Checking session…"}</p> : null}
      {error ? <p className="settings-reset-error" role="alert">{error}</p> : null}
      <footer>
        <button disabled={pending} onClick={() => setStage(undefined)} type="button">Cancel</button>
        <button className="danger" disabled={pending}
          onClick={() => void apply()} type="button">
          {error ? (preview ? "Retry Delete" : "Retry") : stage === "delete" && preview?.active ? "Continue" : "Delete session"}
        </button>
      </footer>
    </PopupDialog>,
  };
}
