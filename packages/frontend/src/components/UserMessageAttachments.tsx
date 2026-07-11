import { Code2 } from "lucide-react";
import { attachmentImageLayout, type AttachmentImagePreviewSource } from "./AttachmentImagePreview";

export interface UserMessageAttachmentItem {
  id: string;
  image?: AttachmentImagePreviewSource;
  label: string;
}

/** Keeps optimistic and persisted user-message attachments on the same visual contract. */
export function UserMessageAttachments({
  attachments,
  onOpenImage,
}: {
  attachments: UserMessageAttachmentItem[];
  onOpenImage: (image: AttachmentImagePreviewSource) => void;
}) {
  const imageAttachments = attachments.filter(
    (item): item is UserMessageAttachmentItem & { image: AttachmentImagePreviewSource } => Boolean(item.image),
  );
  const fileAttachments = attachments.filter((item) => !item.image);

  return (
    <>
      {imageAttachments.length > 0 ? (
        <div className="chat-image-grid" data-layout={attachmentImageLayout(imageAttachments.length)}>
          {imageAttachments.map(({ id, image, label }) => (
            <button
              aria-label={`Open ${label}`}
              className="chat-image-attachment"
              key={id}
              onClick={() => onOpenImage(image)}
              type="button"
            >
              <img className="chat-image-preview" src={image.url} alt={`${label} preview`} />
            </button>
          ))}
        </div>
      ) : null}
      {fileAttachments.length > 0 ? (
        <div className="chat-file-attachments" aria-label="Attached files">
          {fileAttachments.map(({ id, label }) => (
            <span className="chat-attachment-chip" key={id}>
              <Code2 size={11} />
              {label}
            </span>
          ))}
        </div>
      ) : null}
    </>
  );
}
