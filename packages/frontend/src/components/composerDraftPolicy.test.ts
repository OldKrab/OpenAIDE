import { describe, expect, it } from "vitest";
import { composerErrorMessage } from "./composerDraftPolicy";

describe("composerErrorMessage", () => {
  it("keeps concise actionable errors", () => {
    expect(composerErrorMessage(new Error("Attachment is too large."), "Unable to upload image."))
      .toBe("Attachment is too large.");
  });

  it("does not render an HTML gateway response", () => {
    const response = "App Server upload failed with HTTP 403: <!doctype html><html><body>proxy details</body></html>";
    expect(composerErrorMessage(new Error(response), "Unable to upload image."))
      .toBe("Unable to upload image.");
  });
});
