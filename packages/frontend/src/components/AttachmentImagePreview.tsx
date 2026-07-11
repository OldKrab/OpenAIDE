import { X } from "lucide-react";
import { useEffect, useRef } from "react";
import type { Attachment } from "@openaide/app-shell-contracts";
import type { ComposerAttachment } from "../state/composerOptions";

export type AttachmentImagePreviewSource = {
  label: string;
  url: string;
};

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
}: {
  image: AttachmentImagePreviewSource;
  onClose: () => void;
}) {
  const lightboxRef = useRef<HTMLDivElement | null>(null);
  const closeButtonRef = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    if (typeof window === "undefined") return undefined;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onClose();
        return;
      }
      if (event.key === "Tab") {
        event.preventDefault();
        closeButtonRef.current?.focus();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);
  useEffect(() => {
    if (typeof document === "undefined") return undefined;
    const previouslyFocused = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
    closeButtonRef.current?.focus();
    return () => previouslyFocused?.focus();
  }, []);

  return (
    <div
      aria-label={`${image.label} preview`}
      aria-modal="true"
      className="attachment-preview-backdrop"
      onClick={onClose}
      role="dialog"
    >
      <div
        className="attachment-preview-lightbox"
        onClick={(event) => event.stopPropagation()}
        ref={lightboxRef}
        tabIndex={-1}
      >
        <button
          aria-label="Close image preview"
          className="attachment-preview-close"
          onClick={onClose}
          ref={closeButtonRef}
          type="button"
        >
          <X aria-hidden="true" size={18} />
        </button>
        <div className="attachment-preview-stage">
          <img alt={image.label} src={image.url} />
        </div>
      </div>
    </div>
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
