/** Builds the ordinary Composer text inserted when Chat text is quoted. */
export function appendQuoteToDraft(draft: string, selectedText: string) {
  const quote = selectedText
    .split(/\r?\n/)
    .map((line) => line === "" ? ">" : `> ${line}`)
    .join("\n");
  const separator = draft === ""
    ? ""
    : draft.endsWith("\n") ? ""
      : "\n";
  return `${draft}${separator}${quote}\n`;
}

/**
 * Quotes deliberately preserve rendered layout, but very short selections are
 * usually accidental. Count user-perceived, non-whitespace characters so an
 * emoji sequence remains one visible symbol.
 */
export function quoteableSelectionText(selectionText: string) {
  const trimmed = selectionText.trim();
  if (visibleGraphemeCount(trimmed) < 3) return undefined;
  return trimmed;
}

const graphemeSegmenter = new Intl.Segmenter(undefined, { granularity: "grapheme" });

function visibleGraphemeCount(text: string) {
  let count = 0;
  for (const { segment } of graphemeSegmenter.segment(text)) {
    if (/\S/u.test(segment)) count += 1;
  }
  return count;
}
