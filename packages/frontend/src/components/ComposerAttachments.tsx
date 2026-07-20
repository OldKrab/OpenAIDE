import { ExternalLink, FolderOpen, X } from "lucide-react";
import { useState } from "react";
import type { ComposerAttachment } from "../state/composerOptions";
import {
  attachmentImageLayout,
  AttachmentImagePreviewLightbox,
  composerImagePreview,
  type AttachmentImagePreviewSource,
} from "./AttachmentImagePreview";
import { FileKindIcon } from "./ComposerFileMentions";

export type ComposerFileUpload = {
  id: string;
  label: string;
  loaded: number;
  total: number;
  state: "queued" | "uploading" | "error";
  error?: string;
  cancellable?: boolean;
  cancel(): void;
  dismiss(): void;
};

export function ComposerAttachments({
  attachments,
  disabled,
  onRemoveAttachment,
  onRevealAttachment,
  uploads = [],
}: {
  attachments: ComposerAttachment[];
  disabled: boolean;
  onRemoveAttachment: (attachmentId: string) => void;
  onRevealAttachment?: (attachmentId: string) => Promise<void> | void;
  uploads?: ComposerFileUpload[];
}) {
  const [openImage, setOpenImage] = useState<AttachmentImagePreviewSource | undefined>();
  const [revealFeedback, setRevealFeedback] = useState<Record<string, "pending" | "requested" | "failed">>({});
  if (attachments.length === 0 && uploads.length === 0) return null;
  const attachmentItems = attachments.map((attachment) => ({
    attachment,
    image: composerImagePreview(attachment),
  }));
  return (
    <div className="composer-attachments" aria-label="Attached context">
      <div className="composer-attachment-list" data-layout={attachmentImageLayout(attachmentItems.length + uploads.length)}>
        {attachmentItems.map(({ attachment, image }) => image ? (
            <span
              className="composer-attachment-tile composer-image-attachment"
              key={attachment.local_id}
              title={attachment.label}
            >
              <button
                aria-label={`Open ${attachment.label}`}
                className="composer-image-open"
                onClick={() => setOpenImage(image)}
                type="button"
              >
                <img className="composer-image-preview" src={image.url} alt={`${attachment.label} preview`} />
              </button>
              <button
                aria-label={`Remove ${attachment.label}`}
                className="composer-image-remove"
                disabled={disabled}
                onClick={() => onRemoveAttachment(attachment.local_id)}
                type="button"
              >
                <X size={13} />
              </button>
            </span>
          ) : (
            <span
              className="composer-attachment-tile composer-file-attachment"
              key={attachment.local_id}
              title={attachment.label}
            >
              <span className="composer-file-attachment-main">
                {attachment.kind === "file"
                  ? <FileKindIcon path={attachment.label} size={20} />
                  : <FolderOpen size={20} />}
                <span className="composer-file-attachment-label">{attachment.label}</span>
              </span>
              {attachment.app_server_handle_id && onRevealAttachment ? (
                <button
                  aria-label={`Reveal ${attachment.label}`}
                  className="composer-file-reveal"
                  disabled={disabled || revealFeedback[attachment.local_id] === "pending"}
                  onClick={async () => {
                    setRevealFeedback((current) => ({ ...current, [attachment.local_id]: "pending" }));
                    try {
                      await onRevealAttachment(attachment.local_id);
                      setRevealFeedback((current) => ({ ...current, [attachment.local_id]: "requested" }));
                    } catch {
                      setRevealFeedback((current) => ({ ...current, [attachment.local_id]: "failed" }));
                    }
                  }}
                  type="button"
                >
                  <ExternalLink size={11} />
                </button>
              ) : null}
              {revealFeedback[attachment.local_id] === "pending" ? (
                <span className="context-token-status">Revealing...</span>
              ) : revealFeedback[attachment.local_id] === "requested" ? (
                <span className="context-token-status">Reveal requested.</span>
              ) : revealFeedback[attachment.local_id] === "failed" ? (
                <span className="context-token-status error">Unable to reveal {attachment.label}.</span>
              ) : attachment.validation_error ? (
                <span className="context-token-status error">Reselect attachment.</span>
              ) : null}
              <button
                aria-label={`Remove ${attachment.label}`}
                className="composer-file-remove"
                disabled={disabled}
                onClick={() => onRemoveAttachment(attachment.local_id)}
                type="button"
              >
                <X size={11} />
              </button>
            </span>
          ))}
        {uploads.map((upload) => (
          <span
            className="composer-attachment-tile composer-file-attachment composer-file-upload"
            data-state={upload.state}
            key={upload.id}
            title={upload.state === "error" ? upload.error : undefined}
          >
            <span className="composer-file-attachment-main">
              <FileKindIcon path={upload.label} size={20} />
              <span className="composer-file-attachment-label" title={upload.label}>{upload.label}</span>
            </span>
            {upload.state !== "error" ? (
              <progress
                aria-label={`Uploading ${upload.label}`}
                max={Math.max(upload.total, 1)}
                value={upload.loaded}
              />
            ) : null}
            {upload.state === "error" || upload.cancellable !== false ? (
              <button
                aria-label={`${upload.state === "error" ? "Dismiss" : "Cancel"} ${upload.label}`}
                className="composer-file-remove"
                onClick={upload.state === "error" ? upload.dismiss : upload.cancel}
                type="button"
              >
                <X size={11} />
              </button>
            ) : null}
          </span>
        ))}
      </div>
      {openImage ? <AttachmentImagePreviewLightbox image={openImage} onClose={() => setOpenImage(undefined)} /> : null}
    </div>
  );
}
