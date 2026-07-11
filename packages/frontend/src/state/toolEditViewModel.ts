import type { ActivityToolContent, ActivityToolDetails } from "@openaide/app-shell-contracts";
import { splitToolLines } from "./toolCommandViewModel";

export type EditDetailLine =
  | {
      kind: "add" | "remove" | "context";
      oldLineNumber?: number;
      newLineNumber?: number;
      prefix: string;
      text: string;
    }
  | { kind: "hunk"; oldStart: number; oldCount: number; newStart: number; newCount: number; text: string }
  | { kind: "omitted"; count: number; text: string };

export type UnifiedDiffLine = { kind: "context" | "add" | "remove"; text: string };

type PositionedDiffLine = UnifiedDiffLine & {
  oldPosition: number;
  newPosition: number;
  oldLineNumber?: number;
  newLineNumber?: number;
};

type HunkRange = { start: number; end: number };

const MAX_RENDERED_EDIT_DIFF_LINES = 400;
const MAX_MYERS_EDIT_DISTANCE = 400;
const DIFF_CONTEXT_LINES = 3;
const TRUNCATED_DIFF_NOTICE = "Diff truncated. Open the file for the full change.";

export function firstDiffContent(details: ActivityToolDetails) {
  return details.content?.find((content): content is Extract<ActivityToolContent, { kind: "diff" }> => content.kind === "diff");
}

export function editDiffLines(diff: Extract<ActivityToolContent, { kind: "diff" }>): EditDetailLine[] {
  return cappedEditRows(buildEditRows(diff.old_text, diff.new_text));
}

export function editResultText(details: ActivityToolDetails, path: string, failed: boolean, fallbackPreview?: string) {
  const stderr = details.output?.stderr?.trim();
  if (failed && stderr) return stderr;
  if (failed && fallbackPreview) return fallbackPreview;
  const relativePath = path.split("/").filter(Boolean).slice(-2).join("/");
  const diff = firstDiffContent(details);
  return `${diff?.old_text === undefined ? "Created" : "Updated"} ${relativePath || path || "file"}`;
}

export function buildUnifiedDiff(oldText: string | undefined, newText: string): UnifiedDiffLine[] {
  const lines = buildEditRows(oldText, newText).flatMap<UnifiedDiffLine>((row) => {
    if (row.kind === "hunk") return [];
    if (row.kind === "omitted") return [{ kind: "context", text: row.text }];
    return [{ kind: row.kind, text: row.text }];
  });
  return cappedDiffLines(lines);
}

function buildRawUnifiedDiff(oldText: string | undefined, newText: string): UnifiedDiffLine[] {
  if (oldText === undefined) {
    return splitToolLines(newText).map((text) => ({ kind: "add" as const, text }));
  }
  const oldLines = splitToolLines(oldText);
  const newLines = splitToolLines(newText);
  if (oldLines.length * newLines.length > 40_000) {
    return buildMyersDiff(oldLines, newLines) ?? buildBoundedReplacementDiff(oldLines, newLines);
  }
  const table = Array.from({ length: oldLines.length + 1 }, () => Array(newLines.length + 1).fill(0) as number[]);
  for (let oldIndex = oldLines.length - 1; oldIndex >= 0; oldIndex -= 1) {
    for (let newIndex = newLines.length - 1; newIndex >= 0; newIndex -= 1) {
      table[oldIndex][newIndex] =
        oldLines[oldIndex] === newLines[newIndex]
          ? table[oldIndex + 1][newIndex + 1] + 1
          : Math.max(table[oldIndex + 1][newIndex], table[oldIndex][newIndex + 1]);
    }
  }
  const lines: UnifiedDiffLine[] = [];
  let oldIndex = 0;
  let newIndex = 0;
  while (oldIndex < oldLines.length && newIndex < newLines.length) {
    if (oldLines[oldIndex] === newLines[newIndex]) {
      lines.push({ kind: "context", text: oldLines[oldIndex] });
      oldIndex += 1;
      newIndex += 1;
    } else if (table[oldIndex + 1][newIndex] > table[oldIndex][newIndex + 1]) {
      lines.push({ kind: "remove", text: oldLines[oldIndex] });
      oldIndex += 1;
    } else if (table[oldIndex][newIndex + 1] > table[oldIndex + 1][newIndex]) {
      lines.push({ kind: "add", text: newLines[newIndex] });
      newIndex += 1;
    } else {
      lines.push({ kind: "remove", text: oldLines[oldIndex] });
      lines.push({ kind: "add", text: newLines[newIndex] });
      oldIndex += 1;
      newIndex += 1;
    }
  }
  while (oldIndex < oldLines.length) {
    lines.push({ kind: "remove", text: oldLines[oldIndex] });
    oldIndex += 1;
  }
  while (newIndex < newLines.length) {
    lines.push({ kind: "add", text: newLines[newIndex] });
    newIndex += 1;
  }
  return lines;
}

function buildMyersDiff(oldLines: string[], newLines: string[]): UnifiedDiffLine[] | undefined {
  const trace: Array<Map<number, number>> = [];
  const furthestX = new Map<number, number>([[1, 0]]);
  const maxDistance = Math.min(oldLines.length + newLines.length, MAX_MYERS_EDIT_DISTANCE);

  for (let distance = 0; distance <= maxDistance; distance += 1) {
    trace.push(new Map(furthestX));
    for (let diagonal = -distance; diagonal <= distance; diagonal += 2) {
      const moveDown =
        diagonal === -distance ||
        (diagonal !== distance && (furthestX.get(diagonal - 1) ?? -1) < (furthestX.get(diagonal + 1) ?? -1));
      let oldIndex = moveDown ? (furthestX.get(diagonal + 1) ?? 0) : (furthestX.get(diagonal - 1) ?? 0) + 1;
      let newIndex = oldIndex - diagonal;
      while (oldIndex < oldLines.length && newIndex < newLines.length && oldLines[oldIndex] === newLines[newIndex]) {
        oldIndex += 1;
        newIndex += 1;
      }
      furthestX.set(diagonal, oldIndex);
      if (oldIndex >= oldLines.length && newIndex >= newLines.length) {
        return backtrackMyersDiff(trace, oldLines, newLines);
      }
    }
  }
  return undefined;
}

function backtrackMyersDiff(trace: Array<Map<number, number>>, oldLines: string[], newLines: string[]): UnifiedDiffLine[] {
  const reversed: UnifiedDiffLine[] = [];
  let oldIndex = oldLines.length;
  let newIndex = newLines.length;

  for (let distance = trace.length - 1; distance >= 0; distance -= 1) {
    const furthestX = trace[distance];
    const diagonal = oldIndex - newIndex;
    const moveDown =
      diagonal === -distance ||
      (diagonal !== distance && (furthestX.get(diagonal - 1) ?? -1) < (furthestX.get(diagonal + 1) ?? -1));
    const previousDiagonal = moveDown ? diagonal + 1 : diagonal - 1;
    const previousOldIndex = furthestX.get(previousDiagonal) ?? 0;
    const previousNewIndex = previousOldIndex - previousDiagonal;

    while (oldIndex > previousOldIndex && newIndex > previousNewIndex) {
      reversed.push({ kind: "context", text: oldLines[oldIndex - 1] });
      oldIndex -= 1;
      newIndex -= 1;
    }
    if (distance === 0) break;
    if (oldIndex === previousOldIndex) {
      reversed.push({ kind: "add", text: newLines[newIndex - 1] });
      newIndex -= 1;
    } else {
      reversed.push({ kind: "remove", text: oldLines[oldIndex - 1] });
      oldIndex -= 1;
    }
  }
  return reversed.reverse();
}

function addLinePositions(lines: UnifiedDiffLine[]): PositionedDiffLine[] {
  let oldLineNumber = 1;
  let newLineNumber = 1;
  return lines.map((line) => {
    const positioned = { ...line, oldPosition: oldLineNumber, newPosition: newLineNumber };
    if (line.kind === "context") {
      oldLineNumber += 1;
      newLineNumber += 1;
      return { ...positioned, oldLineNumber: oldLineNumber - 1, newLineNumber: newLineNumber - 1 };
    }
    if (line.kind === "remove") {
      oldLineNumber += 1;
      return { ...positioned, oldLineNumber: oldLineNumber - 1 };
    }
    newLineNumber += 1;
    return { ...positioned, newLineNumber: newLineNumber - 1 };
  });
}

function buildEditRows(oldText: string | undefined, newText: string): EditDetailLine[] {
  const lines = addLinePositions(buildRawUnifiedDiff(oldText, newText));
  const hunks = findHunkRanges(lines);
  const rows: EditDetailLine[] = [];
  let previousEnd = 0;
  for (const hunk of hunks) {
    if (hunk.start > previousEnd) rows.push(omittedRow(hunk.start - previousEnd));
    const hunkLines = lines.slice(hunk.start, hunk.end);
    rows.push(hunkHeader(hunkLines), ...hunkLines.map(editLine));
    previousEnd = hunk.end;
  }
  if (previousEnd < lines.length && hunks.length) rows.push(omittedRow(lines.length - previousEnd));
  return rows;
}

function findHunkRanges(lines: PositionedDiffLine[]): HunkRange[] {
  const hunks: HunkRange[] = [];
  for (let index = 0; index < lines.length; ) {
    if (lines[index].kind === "context") {
      index += 1;
      continue;
    }
    const changeStart = index;
    while (index < lines.length && lines[index].kind !== "context") index += 1;
    const range = {
      start: Math.max(0, changeStart - DIFF_CONTEXT_LINES),
      end: Math.min(lines.length, index + DIFF_CONTEXT_LINES),
    };
    const previous = hunks.at(-1);
    if (previous && range.start <= previous.end) {
      previous.end = Math.max(previous.end, range.end);
    } else {
      hunks.push(range);
    }
  }
  return hunks;
}

function hunkHeader(lines: PositionedDiffLine[]): Extract<EditDetailLine, { kind: "hunk" }> {
  const oldCount = lines.filter((line) => line.kind !== "add").length;
  const newCount = lines.filter((line) => line.kind !== "remove").length;
  const oldStart = oldCount === 0 ? Math.max(0, lines[0].oldPosition - 1) : lines[0].oldPosition;
  const newStart = newCount === 0 ? Math.max(0, lines[0].newPosition - 1) : lines[0].newPosition;
  return { kind: "hunk", oldStart, oldCount, newStart, newCount, text: `@@ -${oldStart},${oldCount} +${newStart},${newCount} @@` };
}

function editLine(line: PositionedDiffLine): Extract<EditDetailLine, { kind: "add" | "remove" | "context" }> {
  return {
    kind: line.kind,
    ...(line.oldLineNumber === undefined ? {} : { oldLineNumber: line.oldLineNumber }),
    ...(line.newLineNumber === undefined ? {} : { newLineNumber: line.newLineNumber }),
    prefix: line.kind === "add" ? "+" : line.kind === "remove" ? "-" : " ",
    text: line.text,
  };
}

function omittedRow(count: number): Extract<EditDetailLine, { kind: "omitted" }> {
  return { kind: "omitted", count, text: `${count} unchanged ${count === 1 ? "line" : "lines"}` };
}

function buildBoundedReplacementDiff(oldLines: string[], newLines: string[]): UnifiedDiffLine[] {
  let prefixLength = 0;
  while (prefixLength < oldLines.length && prefixLength < newLines.length && oldLines[prefixLength] === newLines[prefixLength]) {
    prefixLength += 1;
  }

  let suffixLength = 0;
  while (
    suffixLength < oldLines.length - prefixLength &&
    suffixLength < newLines.length - prefixLength &&
    oldLines[oldLines.length - suffixLength - 1] === newLines[newLines.length - suffixLength - 1]
  ) {
    suffixLength += 1;
  }

  const prefix = oldLines.slice(0, prefixLength).map((text) => ({ kind: "context" as const, text }));
  const oldEnd = oldLines.length - suffixLength;
  const newEnd = newLines.length - suffixLength;
  const replacement = buildReplacementDiff(oldLines.slice(prefixLength, oldEnd), newLines.slice(prefixLength, newEnd));
  const suffix = oldLines.slice(oldEnd).map((text) => ({ kind: "context" as const, text }));
  return [...prefix, ...replacement, ...suffix];
}

function buildReplacementDiff(oldLines: string[], newLines: string[]): UnifiedDiffLine[] {
  const lines: UnifiedDiffLine[] = [];
  const pairCount = Math.min(oldLines.length, newLines.length);
  for (let index = 0; index < pairCount; index += 1) {
    lines.push({ kind: "remove", text: oldLines[index] });
    lines.push({ kind: "add", text: newLines[index] });
  }
  for (let index = pairCount; index < oldLines.length; index += 1) {
    lines.push({ kind: "remove", text: oldLines[index] });
  }
  for (let index = pairCount; index < newLines.length; index += 1) {
    lines.push({ kind: "add", text: newLines[index] });
  }
  return lines;
}

function cappedDiffLines<T extends UnifiedDiffLine>(lines: T[]): Array<T | UnifiedDiffLine> {
  if (lines.length <= MAX_RENDERED_EDIT_DIFF_LINES) return lines;
  return [
    ...lines.slice(0, MAX_RENDERED_EDIT_DIFF_LINES),
    { kind: "context" as const, text: TRUNCATED_DIFF_NOTICE },
  ];
}

function cappedEditRows(rows: EditDetailLine[]): EditDetailLine[] {
  if (rows.length <= MAX_RENDERED_EDIT_DIFF_LINES) return rows;
  const omittedCount = rows.length - MAX_RENDERED_EDIT_DIFF_LINES;
  return [...rows.slice(0, MAX_RENDERED_EDIT_DIFF_LINES), { kind: "omitted", count: omittedCount, text: TRUNCATED_DIFF_NOTICE }];
}
