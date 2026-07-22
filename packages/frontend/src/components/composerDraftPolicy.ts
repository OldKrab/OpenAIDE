export function hasComposerContent(value: string, attachmentCount: number) {
  return value.trim().length > 0 || attachmentCount > 0;
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
