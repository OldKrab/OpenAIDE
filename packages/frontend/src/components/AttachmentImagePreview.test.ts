import { describe, expect, it } from "vitest";
import type { AttachmentHandleId } from "@openaide/app-server-client";
import { chatImagePreview, composerImagePreview } from "./AttachmentImagePreview";

describe("attachment image preview sources", () => {
  it("uses composer image preview URLs without changing the accessible label", () => {
    expect(
      composerImagePreview({
        local_id: "attachment_1",
        kind: "file",
        label: "image.png",
        app_server_handle_id: "attachment-handle-1" as AttachmentHandleId,
        preview_url: "data:image/png;base64,aW1hZ2U=",
      }),
    ).toEqual({ label: "image.png", url: "data:image/png;base64,aW1hZ2U=" });
  });

  it("uses only safe image payloads from chat attachments", () => {
    expect(chatImagePreview({ kind: "file", label: "image.png", payload: { preview_url: "data:image/png;base64,aW1hZ2U=" } })).toEqual({
      label: "image.png",
      url: "data:image/png;base64,aW1hZ2U=",
    });

    expect(chatImagePreview({ kind: "file", label: "remote.png", payload: { preview_url: "https://example.com/image.png" } })).toBeUndefined();
    expect(chatImagePreview({ kind: "file", label: "notes.md", payload: { data: "bm90ZXM=", mimeType: "text/plain" } })).toBeUndefined();
  });
});
