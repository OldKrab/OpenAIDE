import { describe, expect, it } from "vitest";
import { composerDraftTexts, composerErrorMessage, hasUnsentComposerInput } from "./composerDraftPolicy";

const empty = { prompt: "", context: [] };
const state = (newTask: object, taskInputs: object = {}) => ({ newTask: { ...empty, ...newTask }, taskInputs }) as never;

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

describe("what a computer switch keeps", () => {
  it("does not count draft text as input it would lose", () => {
    expect(hasUnsentComposerInput(state({ prompt: "text" }, { "task-1": { ...empty, prompt: "text" } }))).toBe(false);
  });

  it("counts attachments and unanswered sends", () => {
    expect(hasUnsentComposerInput(state({ context: [{}] }))).toBe(true);
    expect(hasUnsentComposerInput(state({}, { "task-1": { ...empty, pending: { prompt: "sent" } } }))).toBe(true);
  });

  it("keeps the text of each composer and skips blank ones", () => {
    expect(composerDraftTexts(state({ prompt: "new" }, { "task-1": { ...empty, prompt: "first" }, "task-2": { ...empty, prompt: "  " } })))
      .toEqual({ newTask: "new", tasks: { "task-1": "first" } });
  });

  it("leaves out a send that may yet be accepted", () => {
    expect(composerDraftTexts(state({ prompt: "new", pending: {} }, { "task-1": { ...empty, prompt: "first", pending: {} } }))).toEqual({ tasks: {} });
  });
});
