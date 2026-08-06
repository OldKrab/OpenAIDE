/** Separates complete bold Thought chunks that ACP delivered without whitespace. */
export function presentThoughtMarkdown(text: string) {
  return text.replace(/\*{4}/g, "**\n\n**");
}
