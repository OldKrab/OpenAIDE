import type { ActivityStep, ActivityToolDetails, ActivityToolInput } from "@openaide/app-shell-contracts";
import { displayCommand, splitToolLines } from "./toolCommandViewModel";
import { firstFieldValue, firstToolPath, primaryOutput } from "./toolDetailsShared";

export type SearchDetailMatch = {
  displayPath: string;
  lineNumber: number;
  path: string;
  text: string;
};

export type SearchFileResult = { displayPath: string; path: string };

export type SearchResult = { displayPath: string; line: number; path: string; text: string };

export function searchDetailInfo(
  details: ActivityToolDetails,
  step: Extract<ActivityStep, { kind: "tool" }>,
  fallbackPreview?: string,
) {
  const command = displayCommand(details.input?.command);
  const query =
    details.input?.query ??
    firstFieldValue(details.input?.fields, "query") ??
    firstFieldValue(details.input?.fields, "q") ??
    fileQueryFromCommand(command) ??
    searchQueryFromTitle(step.input_summary ?? "") ??
    "text";
  const path =
    details.input?.path ??
    firstToolPath(details)?.path ??
    firstFieldValue(details.input?.fields, "path") ??
    firstFieldValue(details.input?.fields, "file") ??
    ".";
  const output = primaryOutput(details.output) ?? fallbackPreview ?? "";
  const matches = parseSearchMatches(output, details.input?.cwd);
  const fileResults = matches.length ? [] : parseSearchFileResults(output, details.input?.cwd);
  const exitCode = details.output?.exit_code;
  const mode = matches.length || !isFileFindCommand(command, details.input) ? "text" : "files";

  return {
    command,
    exitCode,
    fileResults,
    matchCount: matches.length || fileResults.length,
    matches,
    mode,
    output,
    path,
    query,
  };
}

export function caretLine(text: string, query: string) {
  if (!query) return "";
  const index = text.toLowerCase().indexOf(query.toLowerCase());
  if (index < 0) return "";
  return `${" ".repeat(index)}${"^".repeat(query.length)}`;
}

export function parseSearchResults(output: string, basePath?: string) {
  return splitToolLines(output)
    .map((line) => {
      const match = /^(.+?):(\d+)(?::\d+)?:([\s\S]*)$/.exec(line);
      if (!match) return undefined;
      const [, displayPath, lineNumber, text] = match;
      return {
        displayPath,
        line: Number(lineNumber),
        path: openablePath(displayPath, basePath),
        text: text.trim(),
      };
    })
    .filter((result): result is SearchResult => Boolean(result));
}

export function openablePath(path: string, basePath?: string) {
  if (path.startsWith("/")) return path;
  const base = basePath?.replace(/\/+$/, "");
  return base ? `${base}/${path}` : path;
}

function parseSearchMatches(output: string, basePath?: string): SearchDetailMatch[] {
  return splitToolLines(output)
    .map((line) => {
      const match = /^(.+?):(\d+)(?::\d+)?:([\s\S]*)$/.exec(line);
      if (!match) return undefined;
      const [, displayPath, lineNumber, text] = match;
      return {
        displayPath,
        lineNumber: Number(lineNumber),
        path: openablePath(displayPath, basePath),
        text,
      };
    })
    .filter((match): match is SearchDetailMatch => Boolean(match));
}

function parseSearchFileResults(output: string, basePath?: string): SearchFileResult[] {
  return splitToolLines(output)
    .map((line) => line.trim())
    .filter((line) => line && !line.includes("command not found") && !line.startsWith("exit "))
    .map((displayPath) => ({ displayPath, path: openablePath(displayPath, basePath) }));
}

function fileQueryFromCommand(command?: string) {
  if (!command) return undefined;
  return /(?:^|\s)-g\s+['"]?([^'"\s]+)['"]?/.exec(command)?.[1] ?? /(?:^|\s)-name\s+['"]?([^'"\s]+)['"]?/.exec(command)?.[1];
}

function searchQueryFromTitle(title: string) {
  const quoted = /Search\s+(.+?)\s+in\s+/i.exec(title);
  return quoted?.[1];
}

function isFileFindCommand(command: string | undefined, input: ActivityToolInput | undefined) {
  const type = firstFieldValue(input?.fields, "type");
  return type === "list_files" || Boolean(command && (/\brg\s+--files\b/.test(command) || /\bfind\s+.+\s-name\s+/.test(command)));
}
