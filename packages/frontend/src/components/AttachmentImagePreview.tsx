import type { Attachment } from "@openaide/app-shell-contracts";
import type { ComposerAttachment } from "../state/composerOptions";
import type { ReactNode } from "react";
import {
  ImagePreviewViewport,
  type ImagePreviewViewportSource,
} from "./ImagePreviewViewport";
import { PopupDialog } from "./Popup";

export type AttachmentImagePreviewSource = ImagePreviewViewportSource;

export type AttachmentImageLayout = "single" | "pair" | "many";

/** Image density follows the number of visual attachments in one authored message. */
export function attachmentImageLayout(count: number): AttachmentImageLayout {
  if (count <= 1) return "single";
  if (count === 2) return "pair";
  return "many";
}

export function AttachmentImagePreviewLightbox({
  image,
  onClose,
  contentNoun,
  toolbarActions,
}: {
  image: AttachmentImagePreviewSource;
  onClose: () => void;
  contentNoun?: string;
  toolbarActions?: ReactNode;
}) {
  return (
    <PopupDialog
      backdropClassName="attachment-preview-backdrop"
      className="attachment-preview-dialog"
      label={`${image.label} preview`}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      open
    >
      <div
        className="attachment-preview-lightbox"
        tabIndex={-1}
      >
        <ImagePreviewViewport
          contentNoun={contentNoun}
          image={image}
          onClose={onClose}
          toolbarActions={toolbarActions}
        />
      </div>
    </PopupDialog>
  );
}

export function composerImagePreview(attachment: ComposerAttachment): AttachmentImagePreviewSource | undefined {
  if (!attachment.preview_url) return undefined;
  return { label: attachment.label, url: attachment.preview_url };
}

export function chatImagePreview(attachment: Attachment): AttachmentImagePreviewSource | undefined {
  const previewUrl = payloadString(attachment.payload, "preview_url") ?? payloadString(attachment.payload, "previewUrl");
  if (previewUrl && previewUrl.startsWith("data:image/")) return { label: attachment.label, url: previewUrl };

  const data = payloadString(attachment.payload, "data");
  const mimeType = payloadString(attachment.payload, "mimeType") ?? payloadString(attachment.payload, "mime");
  if (!data || !mimeType?.startsWith("image/")) return undefined;
  return { label: attachment.label, url: `data:${mimeType};base64,${data}` };
}

function payloadString(payload: unknown, key: string) {
  if (!payload || typeof payload !== "object") return undefined;
  const value = (payload as Record<string, unknown>)[key];
  return typeof value === "string" && value.length > 0 ? value : undefined;
}
