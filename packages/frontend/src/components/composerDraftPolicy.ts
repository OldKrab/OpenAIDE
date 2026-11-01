import type { DraftTexts } from "../services/keptDrafts";
import type { AppState } from "../state/store";

export function hasComposerContent(value: string, attachmentCount: number) {
  return value.trim().length > 0 || attachmentCount > 0;
}

/**
 * Whether any composer holds attachments or a send not yet accepted. These live
 * only in this page, so replacing the page loses them; a switch keeps draft text.
 */
export function hasUnsentComposerInput(state: Pick<AppState, "newTask" | "taskInputs">) {
  const unsent = (input: { context: unknown[]; pending?: unknown }) =>
    input.context.length > 0 || Boolean(input.pending);
  return unsent(state.newTask) || Object.values(state.taskInputs).some(unsent);
}

/** The draft text of every composer. A pending send is left out: it may yet be accepted. */
export function composerDraftTexts(state: Pick<AppState, "newTask" | "taskInputs">): DraftTexts {
  const tasks = Object.entries(state.taskInputs)
    .filter(([, input]) => !input.pending && hasComposerText(input.prompt))
    .map(([taskId, input]) => [taskId, input.prompt]);
  const newTask = !state.newTask.pending && hasComposerText(state.newTask.prompt) ? state.newTask.prompt : undefined;
  return { ...(newTask === undefined ? {} : { newTask }), tasks: Object.fromEntries(tasks) };
}

export function hasComposerText(value: string) {
  return value.trim().length > 0;
}

export function pastedImageFiles(clipboardData: DataTransfer | null) {
  if (!clipboardData) return [];
  const itemFiles = Array.from(clipboardData.items)
    .filter((candidate) => candidate.kind === "file" && candidate.type.startsWith("image/"))
    .map((item) => item.getAsFile())
    .filter((file): file is File => Boolean(file));
  if (itemFiles.length > 0) return itemFiles;
  return Array.from(clipboardData.files ?? []).filter((file) => file.type.startsWith("image/"));
}

export function composerErrorMessage(error: unknown, fallback: string) {
  if (!(error instanceof Error)) return fallback;
  const message = error.message.trim();
  if (!message || /<\s*(?:!doctype|html)\b/i.test(message) || message.length > 320) {
    return fallback;
  }
  return message;
}
