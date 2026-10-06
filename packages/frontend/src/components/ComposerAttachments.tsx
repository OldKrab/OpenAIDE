import { Check, CircleAlert, ExternalLink, FolderOpen, GripVertical, LoaderCircle, X } from "lucide-react";
import { useEffect, useRef, useState, type PointerEvent } from "react";
import type { ComposerAttachment } from "../state/composerOptions";
import {
  attachmentImageLayout,
  AttachmentImagePreviewLightbox,
  composerImagePreview,
  type AttachmentImagePreviewSource,
} from "./AttachmentImagePreview";
import { FileKindIcon } from "./ComposerFileMentions";
import { useAttachmentReorder } from "./useAttachmentReorder";

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
  retry?(): void;
};

export function ComposerAttachments({
  agentLabel,
  attachments,
  disabled,
  imageAttachmentsAllowed,
  onMoveAttachment,
  onRemoveAttachment,
  onRevealAttachment,
  uploads = [],
}: {
  agentLabel: string;
  attachments: ComposerAttachment[];
  disabled: boolean;
  imageAttachmentsAllowed: boolean;
  /** Absent when the surrounding draft cannot be reordered. */
  onMoveAttachment?: (attachmentId: string, targetIndex: number) => void;
  onRemoveAttachment: (attachmentId: string) => void;
  onRevealAttachment?: (attachmentId: string) => Promise<void> | void;
  uploads?: ComposerFileUpload[];
}) {
  const [openImage, setOpenImage] = useState<AttachmentImagePreviewSource | undefined>();
  const [revealFeedback, setRevealFeedback] = useState<Record<string, "pending" | "requested" | "failed">>({});
  const revealFeedbackTimers = useRef(new Map<string, ReturnType<typeof setTimeout>>());
  useEffect(() => () => {
    for (const timer of revealFeedbackTimers.current.values()) clearTimeout(timer);
    revealFeedbackTimers.current.clear();
  }, []);
  const reorder = useAttachmentReorder({
    enabled: !disabled,
    items: attachments.map((attachment) => ({ id: attachment.local_id, label: attachment.label })),
    onMove: onMoveAttachment,
  });
  const reveal = async (attachment: ComposerAttachment) => {
    if (!onRevealAttachment) return;
    clearTimeout(revealFeedbackTimers.current.get(attachment.local_id));
    setRevealFeedback((current) => ({ ...current, [attachment.local_id]: "pending" }));
    try {
      await onRevealAttachment(attachment.local_id);
      setRevealFeedback((current) => ({ ...current, [attachment.local_id]: "requested" }));
      revealFeedbackTimers.current.set(attachment.local_id, setTimeout(() => {
        setRevealFeedback((current) => {
          const next = { ...current };
          delete next[attachment.local_id];
          return next;
        });
        revealFeedbackTimers.current.delete(attachment.local_id);
      }, 2_000));
    } catch {
      setRevealFeedback((current) => ({ ...current, [attachment.local_id]: "failed" }));
    }
  };
  if (attachments.length === 0 && uploads.length === 0) return null;
  // The drag preview only rearranges rendering; the draft order changes on drop.
  const attachmentItems = reorder.orderedIds.flatMap((id) => {
    const attachment = attachments.find((candidate) => candidate.local_id === id);
    return attachment ? [{ attachment, image: composerImagePreview(attachment) }] : [];
  });
  const grip = (attachment: ComposerAttachment) => reorder.reorderable ? (
    <button
      aria-label={`Reorder ${attachment.label}`}
      className="composer-attachment-grip"
      title="Drag or use arrow keys to reorder"
      type="button"
      {...reorder.gripProps(attachment.local_id)}
    >
      <GripVertical aria-hidden="true" size={12} />
    </button>
  ) : null;
  const keepPointer = (event: PointerEvent) => event.stopPropagation();
  // One status replaces the kind caption, so the card keeps a stable two-row rhythm.
  const fileStatus = (attachment: ComposerAttachment) => {
    const feedback = revealFeedback[attachment.local_id];
    if (feedback === "pending") {
      return <span className="context-token-status"><LoaderCircle aria-hidden="true" size={11} />Opening…</span>;
    }
    if (feedback === "requested") {
      return <span className="context-token-status success"><Check aria-hidden="true" size={11} />Opened</span>;
    }
    if (feedback === "failed") {
      return (
        <span className="context-token-status error">
          <CircleAlert aria-hidden="true" size={11} />Couldn’t open file
          <button aria-label={`Retry opening ${attachment.label}`} onClick={() => void reveal(attachment)} onPointerDown={keepPointer} type="button">Retry</button>
        </span>
      );
    }
    if (attachment.validation_error) {
      return (
        <span className="context-token-status error">
          <CircleAlert aria-hidden="true" size={11} />
          {attachment.validation_error.toLowerCase().includes("image")
            ? `Images aren’t supported by ${agentLabel}`
            : "File expired · Choose again"}
        </span>
      );
    }
    if (!attachment.app_server_handle_id) {
      return <span className="context-token-status"><LoaderCircle aria-hidden="true" size={11} />Preparing…</span>;
    }
    return undefined;
  };
  const uploadActive = uploads.some((upload) => upload.state !== "error");
  return (
    <div className="composer-attachments" aria-label="Attached context">
      <div
        className="composer-attachment-list"
        data-dragging={reorder.draggingId ? true : undefined}
        data-layout={attachmentImageLayout(attachmentItems.length + uploads.length)}
        data-reorderable={reorder.reorderable ? true : undefined}
        {...reorder.listProps}
      >
        {attachmentItems.map(({ attachment, image }) => image ? (
            <span
              className="composer-attachment-tile composer-image-attachment"
              key={attachment.local_id}
              title={attachment.label}
              {...reorder.tileProps(attachment.local_id)}
            >
              <button
                aria-label={`Open ${attachment.label}`}
                className="composer-image-open"
                onClick={() => setOpenImage(image)}
                type="button"
              >
                <img className="composer-image-preview" draggable={false} src={image.url} alt={`${attachment.label} preview`} />
              </button>
              {grip(attachment)}
              <button
                aria-label={`Remove ${attachment.label}`}
                className="composer-image-remove"
                disabled={disabled}
                onClick={() => onRemoveAttachment(attachment.local_id)}
                onPointerDown={keepPointer}
                type="button"
              >
                <X size={12} />
              </button>
              {!imageAttachmentsAllowed ? (
                <span className="context-token-status error">
                  <CircleAlert aria-hidden="true" size={11} />Images aren’t supported by {agentLabel}
                </span>
              ) : null}
            </span>
          ) : (
            <span
              className="composer-attachment-tile composer-file-attachment"
              key={attachment.local_id}
              title={attachment.label}
              {...reorder.tileProps(attachment.local_id)}
            >
              <span className="composer-file-attachment-main">
                {attachment.kind === "file"
                  ? <FileKindIcon className="composer-file-attachment-icon" path={attachment.label} size={18} />
                  : <span className="file-kind-icon composer-file-attachment-icon"><FolderOpen aria-hidden="true" size={18} /></span>}
                <span className="composer-file-attachment-text">
                  <span className="composer-file-attachment-label">{attachment.label}</span>
                  {fileStatus(attachment) ?? (
                    <span className="composer-file-attachment-kind">{attachmentKindLabel(attachment)}</span>
                  )}
                </span>
              </span>
              {grip(attachment)}
              {attachment.app_server_handle_id && onRevealAttachment ? (
                <button
                  aria-label={`Reveal ${attachment.label}`}
                  className="composer-file-reveal"
                  disabled={disabled || revealFeedback[attachment.local_id] === "pending"}
                  onClick={() => void reveal(attachment)}
                  onPointerDown={keepPointer}
                  type="button"
                >
                  <ExternalLink size={12} />
                </button>
              ) : null}
              <button
                aria-label={`Remove ${attachment.label}`}
                className="composer-file-remove"
                disabled={disabled}
                onClick={() => onRemoveAttachment(attachment.local_id)}
                onPointerDown={keepPointer}
                type="button"
              >
                <X size={12} />
              </button>
            </span>
          ))}
        {uploads.map((upload) => (
          <span
            className="composer-attachment-tile composer-file-attachment composer-file-upload"
            data-state={upload.state}
            key={upload.id}
          >
            <span className="composer-file-attachment-main">
              <FileKindIcon className="composer-file-attachment-icon" path={upload.label} size={18} />
              <span className="composer-file-attachment-text">
                <span className="composer-file-attachment-label" title={upload.label}>{upload.label}</span>
                {upload.state === "error" ? (
                  <span
                    aria-label={upload.error ? `Upload failed: ${upload.error}` : "Upload failed"}
                    aria-live="polite"
                    className="composer-file-upload-error"
                    role="status"
                  >
                    <CircleAlert aria-hidden="true" size={11} />
                    Upload failed
                    {upload.retry ? (
                      <button
                        aria-label={`Retry ${upload.label}`}
                        className="composer-file-upload-retry"
                        disabled={uploadActive}
                        onClick={upload.retry}
                        type="button"
                      >
                        Retry
                      </button>
                    ) : null}
                  </span>
                ) : (
                  <span aria-hidden="true" className="composer-file-attachment-kind">
                    {upload.state === "queued"
                      ? "Waiting…"
                      : `Uploading ${Math.round((upload.loaded / Math.max(upload.total, 1)) * 100)}%`}
                  </span>
                )}
              </span>
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
                <X size={12} />
              </button>
            ) : null}
          </span>
        ))}
      </div>
      {reorder.reorderable ? <span aria-live="polite" className="visually-hidden">{reorder.announcement}</span> : null}
      {openImage ? <AttachmentImagePreviewLightbox image={openImage} onClose={() => setOpenImage(undefined)} /> : null}
    </div>
  );
}

/** Short caption for a file card; the extension survives even when the name is clipped. */
function attachmentKindLabel(attachment: ComposerAttachment) {
  if (attachment.kind !== "file") return "Folder";
  const extension = /[^./\\]\.([A-Za-z0-9]{1,5})$/.exec(attachment.label)?.[1];
  return extension ? extension.toUpperCase() : "File";
}
