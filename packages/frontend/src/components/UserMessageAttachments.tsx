import { Download, Eye, FolderOpen, type LucideIcon } from "lucide-react";
import { attachmentImageLayout, type AttachmentImagePreviewSource } from "./AttachmentImagePreview";
import { FileKindIcon } from "./ComposerFileMentions";

export interface UserMessageAttachmentItem {
  id: string;
  image?: AttachmentImagePreviewSource;
  label: string;
}

/** Keeps optimistic and persisted user-message attachments on the same visual contract. */
export function UserMessageAttachments({
  attachments,
  fileAction,
  onOpenFile,
  onOpenImage,
}: {
  attachments: UserMessageAttachmentItem[];
  fileAction?: "download" | "reveal";
  onOpenFile?: (attachment: UserMessageAttachmentItem, index: number) => void;
  onOpenImage: (image: AttachmentImagePreviewSource) => void;
}) {
  return (
    <div className="chat-attachment-list" data-layout={attachmentImageLayout(attachments.length)}>
      {attachments.map((attachment, index) => attachment.image ? (
            <button
              aria-label={`Open ${attachment.label}`}
              className="chat-image-attachment chat-attachment-interactive"
              data-attachment-tooltip={attachment.label}
              key={attachment.id}
              onClick={() => onOpenImage(attachment.image as AttachmentImagePreviewSource)}
              type="button"
            >
              <img className="chat-image-preview" src={attachment.image.url} alt={`${attachment.label} preview`} />
              <AttachmentAction action="preview" />
            </button>
          ) : onOpenFile && fileAction ? (
            <button
              aria-label={`${fileAction === "download" ? "Download" : "Reveal"} ${attachment.label}`}
              className="chat-attachment-chip chat-attachment-interactive"
              data-attachment-tooltip={attachment.label}
              key={attachment.id}
              onClick={() => onOpenFile(attachment, index)}
              type="button"
            >
              <FileKindIcon className="chat-file-kind-icon" path={attachment.label} size={28} />
              <span className="chat-attachment-label">{attachment.label}</span>
              <AttachmentAction action={fileAction} />
            </button>
          ) : (
            <span className="chat-attachment-chip" key={attachment.id} title={attachment.label}>
              <FileKindIcon className="chat-file-kind-icon" path={attachment.label} size={28} />
              <span className="chat-attachment-label">{attachment.label}</span>
            </span>
          ))}
    </div>
  );
}

function AttachmentAction({ action }: { action: "download" | "preview" | "reveal" }) {
  const actionDetails: Record<typeof action, { Icon: LucideIcon; label: string }> = {
    download: { Icon: Download, label: "Download" },
    preview: { Icon: Eye, label: "Preview" },
    reveal: { Icon: FolderOpen, label: "Reveal" },
  };
  const { Icon, label } = actionDetails[action];
  return (
    <span
      aria-hidden="true"
      className="chat-attachment-action-overlay"
      data-attachment-action={action}
    >
      <Icon size={17} />
      <span>{label}</span>
    </span>
  );
}
