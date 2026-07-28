import { useState } from "react";
import type { ToolImagePreview } from "@openaide/app-server-client";
import { AttachmentImagePreviewLightbox } from "./AttachmentImagePreview";

/** Renders a compact Tool-file thumbnail and delegates inspection to the shared lightbox. */
export function ToolImageFilePreview({ preview }: { preview: ToolImagePreview }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button
        aria-label={`Open image preview for ${preview.label}`}
        className="activity-tool-image-preview"
        onClick={() => setOpen(true)}
        type="button"
      >
        <img alt="" aria-hidden="true" src={preview.dataUrl} />
      </button>
      {open ? (
        <AttachmentImagePreviewLightbox
          image={{ label: preview.label, url: preview.dataUrl }}
          onClose={() => setOpen(false)}
        />
      ) : null}
    </>
  );
}
