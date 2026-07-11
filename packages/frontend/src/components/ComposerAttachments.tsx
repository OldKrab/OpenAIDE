import { ExternalLink, FolderOpen, Paperclip, X } from "lucide-react";
import { useState } from "react";
import type { ComposerAttachment } from "../state/composerOptions";
import {
  attachmentImageLayout,
  AttachmentImagePreviewLightbox,
  composerImagePreview,
  type AttachmentImagePreviewSource,
} from "./AttachmentImagePreview";

export function ComposerAttachments({
  attachments,
  disabled,
  onRemoveAttachment,
  onRevealAttachment,
}: {
  attachments: ComposerAttachment[];
  disabled: boolean;
  onRemoveAttachment: (attachmentId: string) => void;
  onRevealAttachment?: (attachmentId: string) => Promise<void> | void;
}) {
  const [openImage, setOpenImage] = useState<AttachmentImagePreviewSource | undefined>();
  const [revealFeedback, setRevealFeedback] = useState<Record<string, "pending" | "requested" | "failed">>({});
  if (attachments.length === 0) return null;
  const imageAttachments = attachments
    .map((attachment) => ({ attachment, image: composerImagePreview(attachment) }))
    .filter((item): item is { attachment: ComposerAttachment; image: AttachmentImagePreviewSource } => Boolean(item.image));
  const fileAttachments = attachments.filter((attachment) => !composerImagePreview(attachment));
  return (
    <div className="composer-attachments" aria-label="Attached context">
      {imageAttachments.length > 0 ? (
        <div className="composer-image-grid" data-layout={attachmentImageLayout(imageAttachments.length)}>
          {imageAttachments.map(({ attachment, image }) => (
            <span className="composer-image-attachment" key={attachment.local_id}>
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
          ))}
        </div>
      ) : null}
      {fileAttachments.length > 0 ? (
        <div className="composer-file-attachments">
          {fileAttachments.map((attachment) => (
            <span className="context-token" key={attachment.local_id}>
              {attachment.kind === "file" ? <Paperclip size={12} /> : <FolderOpen size={12} />}
              <span className="context-token-label">{attachment.label}</span>
              {attachment.app_server_handle_id && onRevealAttachment ? (
                <button
                  aria-label={`Reveal ${attachment.label}`}
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
                disabled={disabled}
                onClick={() => onRemoveAttachment(attachment.local_id)}
                type="button"
              >
                <X size={11} />
              </button>
            </span>
          ))}
        </div>
      ) : null}
      {openImage ? <AttachmentImagePreviewLightbox image={openImage} onClose={() => setOpenImage(undefined)} /> : null}
    </div>
  );
}
