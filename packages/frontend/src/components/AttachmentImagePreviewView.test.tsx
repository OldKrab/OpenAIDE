import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { AttachmentImagePreviewLightbox } from "./AttachmentImagePreview";

describe("AttachmentImagePreviewLightbox", () => {
  it("keeps full-page screenshots identifiable as an image preview", () => {
    const html = renderToStaticMarkup(
      <AttachmentImagePreviewLightbox
        image={{ label: "diagram.png", url: "data:image/png;base64,aW1hZ2U=" }}
        onClose={vi.fn()}
      />,
    );

    expect(html).toContain('role="dialog"');
    expect(html).toContain('aria-modal="true"');
    expect(html).toContain('aria-label="Close image preview"');
    expect(html).toContain('class="attachment-preview-close"');
    expect(html).toContain('class="attachment-preview-chrome"');
    expect(html).toContain('class="attachment-preview-label"');
    expect(html).not.toContain("attachment-preview-header");
    expect(html).toContain("diagram.png");
    expect(html.indexOf("attachment-preview-chrome")).toBeLessThan(html.indexOf("attachment-preview-stage"));
  });
});
