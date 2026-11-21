import { afterEach, describe, expect, it, vi } from "vitest";
import type { AttachmentHandleId } from "@openaide/app-server-client";
import { composerMessageContent, localImageAttachment, uploadedImageAttachment, type ComposerAttachment } from "./composerOptions";

const handle = (id: string) => id as AttachmentHandleId;

describe("composer Send content", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("sends uploaded Images as handles in draft order beside files, and client-held Images inline", () => {
    vi.stubGlobal("URL", { ...URL, createObjectURL: () => "blob:preview" });
    const photo = uploadedImageAttachment({ handleId: handle("image-1"), label: "photo.jpg" }, new File(["x"], "photo.jpg"));
    const file: ComposerAttachment = { kind: "file", label: "notes.md", local_id: "file-1", app_server_handle_id: handle("file-1") };
    const pasted = localImageAttachment(new File(["x"], "pasted.png", { type: "image/png" }), "eA==");

    expect(photo).toMatchObject({ kind: "image", preview_url: "blob:preview", app_server_handle_id: "image-1" });
    expect(photo.payload).toBeUndefined();
    expect(composerMessageContent([photo, file, pasted])).toEqual({
      images: [{ label: "pasted.png", mimeType: "image/png", data: "eA==" }],
      attachments: ["image-1", "file-1"],
    });
  });

  it("holds Send while an Image has neither a handle nor inline bytes", () => {
    const invalidated: ComposerAttachment = { kind: "image", label: "photo.jpg", local_id: "image-2", validation_error: "Attach it again." };

    expect(composerMessageContent([invalidated])).toBeUndefined();
  });
});
