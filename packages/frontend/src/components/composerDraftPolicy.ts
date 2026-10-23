import type { AppState } from "../state/store";

export function hasComposerContent(value: string, attachmentCount: number) {
  return value.trim().length > 0 || attachmentCount > 0;
}

/**
 * Whether any composer holds text, attachments, or a send not yet accepted.
 * These live only in this page, so replacing the page loses them.
 */
export function hasUnsentComposerInput(state: Pick<AppState, "newTask" | "taskInputs">) {
  const unsent = (input: { prompt: string; context: unknown[]; pending?: unknown }) =>
    hasComposerContent(input.prompt, input.context.length) || Boolean(input.pending);
  return unsent(state.newTask) || Object.values(state.taskInputs).some(unsent);
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
