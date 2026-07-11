import fuzzysort from "fuzzysort";
import type { AgentSlashCommand } from "@openaide/app-shell-contracts";

export type CommandToken = {
  start: number;
  end: number;
  name: string;
};

export type SlashCommandMatch = {
  command: AgentSlashCommand;
  token: CommandToken;
};

type IndexedCommand = {
  command: AgentSlashCommand;
  index: number;
  normalized: string;
  compact: string;
  segments: string[];
};

const MAX_RESULTS = 8;

export function slashCommandPickerResults(commands: AgentSlashCommand[] | undefined, query: string): AgentSlashCommand[] {
  if (!commands?.length) return [];
  const filteredCommands = query.trim().startsWith("$")
    ? commands.filter((command) => command.name.replace(/^\/+/, "").startsWith("$"))
    : commands;
  if (!filteredCommands.length) return [];
  const normalizedQuery = normalizeCommandName(query);
  const compactQuery = compactCommandName(query);
  if (!compactQuery) return filteredCommands.slice(0, MAX_RESULTS);
  const indexed = filteredCommands.map(indexCommand);
  const fuzzyResults = fuzzysort.go(compactQuery, indexed, {
    keys: ["compact", "normalized"],
    limit: MAX_RESULTS * 2,
  });
  return fuzzyResults
    .map((result) => ({
      command: result.obj.command,
      commandIndex: result.obj.index,
      score: commandBoost(result.obj, normalizedQuery, compactQuery) + fuzzyScore(result.score),
    }))
    .sort((a, b) => b.score - a.score || a.commandIndex - b.commandIndex)
    .slice(0, MAX_RESULTS)
    .map((result) => result.command);
}

export function commandTokenAtCursor(text: string, cursor: number): CommandToken | undefined {
  if (cursor < 0 || cursor > text.length) return undefined;
  for (let index = 0; index < text.length; index += 1) {
    if (text[index] !== "/") continue;
    if (index > 0 && !/\s/.test(text[index - 1])) continue;
    let end = index + 1;
    while (end < text.length && commandNameChar(text[end])) end += 1;
    if (end < text.length && !/\s/.test(text[end])) continue;
    if (cursor >= index + 1 && cursor <= end) {
      return { start: index, end, name: text.slice(index + 1, end) };
    }
    index = end;
  }
  return undefined;
}

export function exactSlashCommandMatches(text: string, commands: AgentSlashCommand[] | undefined): SlashCommandMatch[] {
  if (!commands?.length) return [];
  const byName = new Map(commands.map((command) => [canonicalCommandName(command.name), command]));
  return commandTokens(text)
    .map((token) => {
      const command = byName.get(canonicalCommandName(token.name));
      return command ? { command, token } : undefined;
    })
    .filter((match): match is SlashCommandMatch => match !== undefined);
}

export function slashCommandDisplayName(command: AgentSlashCommand) {
  return `/${command.name.replace(/^\/+/, "")}`;
}

function commandTokens(text: string): CommandToken[] {
  const tokens: CommandToken[] = [];
  for (let index = 0; index < text.length; index += 1) {
    if (text[index] !== "/") continue;
    if (index > 0 && !/\s/.test(text[index - 1])) continue;
    let end = index + 1;
    while (end < text.length && commandNameChar(text[end])) end += 1;
    if (end === index + 1) continue;
    if (end < text.length && !/\s/.test(text[end])) continue;
    tokens.push({ start: index, end, name: text.slice(index + 1, end) });
    index = end;
  }
  return tokens;
}

function commandNameChar(char: string | undefined) {
  return char !== undefined && /[A-Za-z0-9$_-]/.test(char);
}

function indexCommand(command: AgentSlashCommand, index: number): IndexedCommand {
  return {
    command,
    index,
    normalized: normalizeCommandName(command.name),
    compact: compactCommandName(command.name),
    segments: commandSegments(command.name),
  };
}

function commandBoost(command: IndexedCommand, normalizedQuery: string, compactQuery: string) {
  if (command.compact === compactQuery) return 1000;
  if (command.compact.startsWith(compactQuery)) return 900 - command.compact.length / 100;
  if (command.segments.some((segment) => segment === compactQuery)) return 800;
  if (command.segments.some((segment) => segment.startsWith(compactQuery))) return 700;
  if (command.compact.includes(compactQuery)) return 600 - command.compact.indexOf(compactQuery);
  if (command.segments.some((segment) => segment.includes(compactQuery))) return 500;
  if (normalizedQuery && command.normalized.includes(normalizedQuery)) return 400;
  return 0;
}

function fuzzyScore(score: number) {
  return Math.max(0, 100 + score / 1000);
}

function canonicalCommandName(name: string) {
  return name.replace(/^\/+/, "").toLowerCase();
}

function normalizeCommandName(name: string) {
  return name
    .replace(/^\/+/, "")
    .replace(/^\$/, "")
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .replace(/[$_.-]+/g, " ")
    .toLowerCase()
    .trim();
}

function compactCommandName(name: string) {
  return normalizeCommandName(name).replace(/\s+/g, "");
}

function commandSegments(name: string) {
  return normalizeCommandName(name).split(/\s+/).filter(Boolean);
}
