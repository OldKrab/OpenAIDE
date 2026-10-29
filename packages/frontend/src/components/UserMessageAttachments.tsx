import { Download, Eye, FolderOpen, type LucideIcon } from "lucide-react";
import { attachmentImageLayout, type AttachmentImagePreviewSource } from "./AttachmentImagePreview";
import { FileKindIcon } from "./ComposerFileMentions";
import {
  storedChatImageSize,
  StoredChatImagePlaceholder,
  useStoredChatImage,
  type StoredChatImageReference,
} from "./StoredChatImage";

export interface UserMessageAttachmentItem {
  id: string;
  /** An image the client holds itself, such as a local preview not yet stored by App Server. */
  image?: AttachmentImagePreviewSource;
  /** A stored Task image (ADR-0075), loaded through the Chat image cache when shown. */
  storedImage?: StoredChatImageReference;
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
            <ImageAttachmentButton image={attachment.image} key={attachment.id} onOpenImage={onOpenImage} />
          ) : attachment.storedImage ? (
            <StoredImageAttachment
              key={attachment.id}
              label={attachment.label}
              onOpenImage={onOpenImage}
              reference={attachment.storedImage}
            />
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

function ImageAttachmentButton({
  image,
  onOpenImage,
  size,
}: {
  image: AttachmentImagePreviewSource;
  onOpenImage: (image: AttachmentImagePreviewSource) => void;
  size?: { width: number; height: number };
}) {
  return (
    <button
      aria-label={`Open ${image.label}`}
      className="chat-image-attachment chat-attachment-interactive"
      data-attachment-tooltip={image.label}
      onClick={() => onOpenImage(image)}
      type="button"
    >
      <img
        alt={`${image.label} preview`}
        className="chat-image-preview"
        height={size?.height}
        src={image.url}
        width={size?.width}
      />
      <AttachmentAction action="preview" />
    </button>
  );
}

/** A sent Image App Server stored for the Task; it loads when shown and opens once loaded. */
function StoredImageAttachment({
  label,
  onOpenImage,
  reference,
}: {
  label: string;
  onOpenImage: (image: AttachmentImagePreviewSource) => void;
  reference: StoredChatImageReference;
}) {
  const state = useStoredChatImage(reference);
  const size = storedChatImageSize(reference);
  if (state.status !== "ready") {
    return <StoredChatImagePlaceholder className="chat-image-attachment" label={label} size={size} state={state} />;
  }
  return <ImageAttachmentButton image={{ label, url: state.url }} onOpenImage={onOpenImage} size={size} />;
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
