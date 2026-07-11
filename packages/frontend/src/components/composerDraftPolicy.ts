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
  return error instanceof Error ? error.message : fallback;
}
