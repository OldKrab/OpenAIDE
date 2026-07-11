import { LoaderCircle } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import type { ComposerAttachment } from "../state/composerOptions";
import {
  AttachmentImagePreviewLightbox,
  composerImagePreview,
  type AttachmentImagePreviewSource,
} from "./AttachmentImagePreview";
import { TaskHeader } from "./TaskHeader";
import { UserMessageAttachments } from "./UserMessageAttachments";

const SLOW_START_DELAY_MS = 5_000;

export function NewTaskStartingView({
  agentId,
  agentName,
  composer,
  openingNativeSession,
  pendingContext,
  pendingPrompt,
  workspaceRoot,
}: {
  agentId: string;
  agentName: string;
  composer: ReactNode;
  openingNativeSession: boolean;
  pendingContext: ComposerAttachment[];
  pendingPrompt: string;
  workspaceRoot: string;
}) {
  const [showSlowStartHint, setShowSlowStartHint] = useState(false);
  const [openImage, setOpenImage] = useState<AttachmentImagePreviewSource | undefined>();
  const statusLabel = openingNativeSession ? "Opening task" : "Starting task";

  useEffect(() => {
    const timeout = globalThis.setTimeout(() => setShowSlowStartHint(true), SLOW_START_DELAY_MS);
    return () => globalThis.clearTimeout(timeout);
  }, []);

  return (
    <section className="task-surface new-task-starting-surface" aria-label={statusLabel}>
      <TaskHeader
        agentId={agentId}
        agentName={agentName}
        status="active"
        statusLabel={openingNativeSession ? "Opening" : "Starting"}
        title="New task"
        workspaceRoot={workspaceRoot}
      />
      <div className="chat-column new-task-starting-chat">
        <div className="message-list-shell">
          <div className="message-list">
            {pendingPrompt.trim() || pendingContext.length ? (
              <div className="chat-user-block" aria-label="Submitted message">
                {pendingContext.length ? (
                  <UserMessageAttachments
                    attachments={pendingContext.map((attachment, index) => ({
                      id: attachment.local_id ?? `${attachment.label}-${index}`,
                      image: composerImagePreview(attachment),
                      label: attachment.label,
                    }))}
                    onOpenImage={setOpenImage}
                  />
                ) : null}
                {pendingPrompt.trim() ? <p className="chat-user">{pendingPrompt}</p> : null}
                {openImage ? <AttachmentImagePreviewLightbox image={openImage} onClose={() => setOpenImage(undefined)} /> : null}
              </div>
            ) : null}
            <div className="new-task-starting-status" role="status" aria-live="polite">
              <LoaderCircle aria-hidden="true" size={14} />
              <span>{statusLabel}…</span>
            </div>
            {showSlowStartHint ? (
              <p className="new-task-slow-start-hint">This is taking longer than usual.</p>
            ) : null}
          </div>
        </div>
        {composer}
      </div>
    </section>
  );
}
