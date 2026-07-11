export type LiveTextPresentation = {
  caughtUp: boolean;
  receivedText: string;
  visibleText: string;
};

const MINIMUM_REVEAL_FRAMES = 10;
const WORD_BOUNDARY_LOOKAHEAD = 12;

export function startLiveText(text: string): LiveTextPresentation {
  return { caughtUp: true, receivedText: text, visibleText: text };
}

export function receiveLiveText(
  state: LiveTextPresentation,
  receivedText: string,
): LiveTextPresentation {
  if (receivedText === state.receivedText) return state;
  if (!receivedText.startsWith(state.receivedText) || !receivedText.startsWith(state.visibleText)) {
    return startLiveText(receivedText);
  }
  return {
    caughtUp: state.visibleText === receivedText,
    receivedText,
    visibleText: state.visibleText,
  };
}

export function advanceLiveText(state: LiveTextPresentation): LiveTextPresentation {
  if (state.caughtUp) return state;
  const visible = graphemes(state.visibleText);
  const received = graphemes(state.receivedText);
  const remaining = received.length - visible.length;
  const evenRevealStep = Math.max(1, Math.ceil(remaining / MINIMUM_REVEAL_FRAMES));
  const revealStep = Math.min(evenRevealStep, maximumRevealStep(remaining));
  const nextCount = wordAwareBoundary(received, visible.length, revealStep);
  const visibleText = received.slice(0, nextCount).join("");
  return {
    caughtUp: visibleText === state.receivedText,
    receivedText: state.receivedText,
    visibleText,
  };
}

// Large backlogs accelerate enough to stay live without collapsing a full mobile screen into a
// handful of frames. Small chunks retain a minimum visible reveal duration.
function maximumRevealStep(remaining: number) {
  if (remaining > 800) return 16;
  if (remaining > 300) return 8;
  if (remaining > 120) return 6;
  return 4;
}

export function stableMarkdownTarget(receivedText: string): string {
  const lineStart = receivedText.lastIndexOf("\n") + 1;
  const currentLine = receivedText.slice(lineStart);
  const uncertainIndex = unfinishedMarkdownIndex(currentLine);
  const inlineBoundary = uncertainIndex === undefined
    ? receivedText
    : receivedText.slice(0, lineStart + uncertainIndex);
  const tableBoundary = unresolvedTableHeaderBoundary(receivedText);
  return tableBoundary === undefined
    ? inlineBoundary
    : inlineBoundary.slice(0, Math.min(inlineBoundary.length, tableBoundary));
}

function graphemes(text: string): string[] {
  if (typeof Intl.Segmenter !== "function") return Array.from(text);
  const segmenter = new Intl.Segmenter(undefined, { granularity: "grapheme" });
  return Array.from(segmenter.segment(text), ({ segment }) => segment);
}

function wordAwareBoundary(received: string[], start: number, minimumStep: number) {
  const minimumEnd = Math.min(received.length, start + minimumStep);
  const maximumEnd = Math.min(received.length, minimumEnd + WORD_BOUNDARY_LOOKAHEAD);
  for (let index = minimumEnd; index < maximumEnd; index += 1) {
    if (/\s/u.test(received[index] ?? "")) return index + 1;
  }
  return minimumEnd;
}

function unfinishedMarkdownIndex(line: string): number | undefined {
  for (let index = 0; index < line.length; index += 1) {
    if (line[index] === "\\") {
      index += 1;
      continue;
    }
    const delimiter = inlineDelimiterAt(line, index);
    if (delimiter) {
      const closing = line.indexOf(delimiter, index + delimiter.length);
      if (closing === -1) return index;
      index = closing + delimiter.length - 1;
      continue;
    }
    if (line[index] === "[" || (line[index] === "!" && line[index + 1] === "[")) {
      const start = index;
      const labelStart = line[index] === "!" ? index + 1 : index;
      const labelEnd = line.indexOf("]", labelStart + 1);
      if (labelEnd === -1) return start;
      if (line[labelEnd + 1] === "(" && line.indexOf(")", labelEnd + 2) === -1) return start;
      index = labelEnd;
      continue;
    }
    if (line[index] === "<" && line.indexOf(">", index + 1) === -1) return index;
  }
  return undefined;
}

function inlineDelimiterAt(line: string, index: number): string | undefined {
  for (const delimiter of ["```", "**", "__", "~~", "`", "*", "_"]) {
    if (line.startsWith(delimiter, index)) return delimiter;
  }
  return undefined;
}

function unresolvedTableHeaderBoundary(text: string): number | undefined {
  const lines = linesWithOffsets(text);
  const lineIsComplete = text.endsWith("\n");
  const candidateIndex = lineIsComplete ? lines.length - 2 : lines.length - 1;
  if (candidateIndex < 0) return undefined;
  const candidate = lines[candidateIndex];
  if (
    !candidate
    || isInsideFence(lines, candidateIndex)
    || !isTableCandidate(candidate.text)
  ) return undefined;
  if (lineIsComplete && isInConfirmedTable(lines, candidateIndex)) return undefined;
  return candidate.start;
}

function linesWithOffsets(text: string) {
  const lines: Array<{ start: number; text: string }> = [];
  let start = 0;
  for (const line of text.split("\n")) {
    lines.push({ start, text: line });
    start += line.length + 1;
  }
  return lines;
}

function isInsideFence(lines: Array<{ text: string }>, lineIndex: number) {
  let fence: "```" | "~~~" | undefined;
  for (let index = 0; index < lineIndex; index += 1) {
    const trimmed = lines[index]?.text.trimStart() ?? "";
    if (!fence && (trimmed.startsWith("```") || trimmed.startsWith("~~~"))) {
      fence = trimmed.startsWith("```") ? "```" : "~~~";
    } else if (fence && trimmed.startsWith(fence)) {
      fence = undefined;
    }
  }
  return fence !== undefined;
}

function isInConfirmedTable(lines: Array<{ text: string }>, lineIndex: number) {
  for (let index = lineIndex - 1; index > 0; index -= 1) {
    const line = lines[index]?.text ?? "";
    if (line.trim() === "") return false;
    if (isTableDelimiter(line)) return isTableCandidate(lines[index - 1]?.text ?? "");
  }
  return false;
}

function isTableCandidate(line: string) {
  return hasUnescapedPipe(line) && !isTableDelimiter(line);
}

function hasUnescapedPipe(line: string) {
  for (let index = 0; index < line.length; index += 1) {
    if (line[index] === "\\") {
      index += 1;
    } else if (line[index] === "|") {
      return true;
    }
  }
  return false;
}

function isTableDelimiter(line: string) {
  return /^\s*\|?\s*:?-{3,}:?\s*(?:\|\s*:?-{3,}:?\s*)+\|?\s*$/u.test(line);
}
