#!/usr/bin/env node
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { homedir } from "node:os";
import { basename, join, resolve } from "node:path";

const command = process.argv[2] ?? "help";
const args = process.argv.slice(3);

function main() {
  if (command === "help" || command === "--help" || command === "-h") return printHelp();
  if (command === "latest") return printLatest(args[0]);

  const { target, flags } = parseTarget(args);
  if (!target) fail(`Usage: acp-trace.mjs ${command} [trace.jsonl|trace-dir]`);
  const tracePath = resolveTraceFile(target);
  const events = readTrace(tracePath);

  if (command === "summary") return printSummary(tracePath, events);
  if (command === "tools") return printTools(tracePath, events);
  if (command === "permissions") return printPermissions(tracePath, events);
  if (command === "raw-index") return printRawIndex(tracePath, events);
  if (command === "show") return printLine(tracePath, events, flags);
  fail(`Unknown command: ${command}`);
}

function printHelp() {
  console.log(`ACP trace helper

Commands:
  latest [dir]              Print newest ACP trace path.
  summary [file|dir]        Count events, directions, session/update kinds.
  tools [file|dir]          Summarize tool_call and tool_call_update lifecycle.
  permissions [file|dir]    Summarize session/request_permission payload shapes.
  raw-index [file|dir]      List raw JSON-RPC directions, methods, ids, and keys.
  show [file|dir] --line N  Show one trace event. Strings are shortened by default.

Flags:
  --full                    With show, print full payloads. Use carefully.

Default lookup:
  Uses OPENAIDE_ACP_TRACE_DIR, then VS Code globalStorage acp-traces dirs.`);
}

function printLatest(input) {
  console.log(resolveTraceFile(input));
}

function printSummary(tracePath, events) {
  const counts = new Map();
  const directions = new Map();
  const rawMethods = new Map();
  const rawSessionUpdates = new Map();
  const typedSessionUpdates = new Map();
  let rawJsonRpcParseErrors = 0;
  let rawStderrLines = 0;

  for (const event of events) {
    increment(counts, event.event ?? "<missing>");
    if (event.direction) increment(directions, event.direction);
    const raw = parseRaw(event);
    if (event.event === "raw_line" && event.direction === "agent_to_client.raw_stderr") rawStderrLines += 1;
    if (event.event === "raw_line" && !raw && event.direction !== "agent_to_client.raw_stderr") rawJsonRpcParseErrors += 1;
    const method = raw?.method ?? (raw?.result ? "<result>" : raw?.error ? "<error>" : undefined);
    if (method) increment(rawMethods, method);
    const rawUpdateKind = raw?.method === "session/update" ? raw.params?.update?.sessionUpdate : undefined;
    if (rawUpdateKind) increment(rawSessionUpdates, rawUpdateKind);
    const typedUpdateKind = event.event === "session/update" ? event.payload?.update?.sessionUpdate : undefined;
    if (typedUpdateKind) increment(typedSessionUpdates, typedUpdateKind);
  }

  console.log(JSON.stringify({
    tracePath,
    sizeBytes: statSync(tracePath).size,
    lines: events.length,
    rawJsonRpcParseErrors,
    rawStderrLines,
    events: sortedEntries(counts),
    directions: sortedEntries(directions),
    rawMethods: sortedEntries(rawMethods),
    rawSessionUpdates: sortedEntries(rawSessionUpdates),
    typedSessionUpdates: sortedEntries(typedSessionUpdates),
  }, null, 2));
}

function printTools(tracePath, events) {
  const calls = new Map();
  for (const { line, raw, event } of preferredSessionUpdates(events)) {
    const update = raw?.params?.update ?? event.payload?.update;
    if (!["tool_call", "tool_call_update"].includes(update?.sessionUpdate)) continue;
    const id = update.toolCallId ?? "<missing>";
    if (!calls.has(id)) calls.set(id, { id, starts: [], updates: [] });
    const record = summarizeToolUpdate(line, update);
    if (update.sessionUpdate === "tool_call") calls.get(id).starts.push(record);
    else calls.get(id).updates.push(record);
  }

  const rows = [...calls.values()].map((call) => {
    const first = call.starts[0] ?? {};
    const last = call.updates.at(-1) ?? {};
    return {
      id: call.id,
      kind: first.kind ?? last.kind ?? "<missing-kind-on-update>",
      title: first.title ?? last.title,
      startLine: first.line,
      finalLine: last.line,
      finalStatus: last.status ?? first.status,
      contentTypes: unique([...(first.contentTypes ?? []), ...(last.contentTypes ?? [])]),
      locationCount: Math.max(first.locationCount ?? 0, last.locationCount ?? 0),
      rawInputKeys: first.rawInputKeys ?? [],
      rawOutputKeys: last.rawOutputKeys ?? [],
      parsedCmdTypes: unique([...(first.parsedCmdTypes ?? []), ...(last.parsedCmdTypes ?? [])]),
      exitCode: last.exitCode,
      stdoutBytes: last.stdoutBytes,
      stderrBytes: last.stderrBytes,
    };
  });

  const byKind = {};
  for (const row of rows) byKind[row.kind] = (byKind[row.kind] ?? 0) + 1;
  console.log(JSON.stringify({ tracePath, totalToolCalls: rows.length, byKind, rows }, null, 2));
}

function printPermissions(tracePath, events) {
  const rows = [];
  for (const { line, raw, event } of preferredPermissionRequests(events)) {
    const payload = raw?.method === "session/request_permission" ? raw.params : event.event === "session/request_permission.request" ? event.payload : undefined;
    if (!payload) continue;
    const toolCall = payload.toolCall ?? {};
    rows.push({
      line,
      direction: event.direction,
      requestId: raw?.id,
      toolCallId: toolCall.toolCallId,
      toolKind: toolCall.kind,
      title: toolCall.title,
      toolKeys: Object.keys(toolCall).sort(),
      optionKinds: (payload.options ?? []).map((option) => option.kind ?? option.optionId ?? Object.keys(option).sort().join(",")),
      optionNames: (payload.options ?? []).map((option) => option.name).filter(Boolean),
    });
  }
  console.log(JSON.stringify({ tracePath, count: rows.length, rows }, null, 2));
}

function printRawIndex(tracePath, events) {
  const rows = [];
  for (const event of events) {
    if (event.event !== "raw_line") continue;
    const raw = parseRaw(event);
    rows.push({
      line: event.line,
      direction: event.direction,
      bytes: Buffer.byteLength(event.payload?.line ?? "", "utf8"),
      method: raw?.method ?? (raw?.result ? "<result>" : raw?.error ? "<error>" : "<parse-error>"),
      id: Object.hasOwn(raw ?? {}, "id") ? raw.id : undefined,
      keys: raw ? Object.keys(raw).sort() : [],
      paramsKeys: raw?.params && typeof raw.params === "object" ? Object.keys(raw.params).sort() : undefined,
      resultKeys: raw?.result && typeof raw.result === "object" ? Object.keys(raw.result).sort() : undefined,
    });
  }
  console.log(JSON.stringify({ tracePath, count: rows.length, rows }, null, 2));
}

function printLine(tracePath, events, flags) {
  const lineNumber = Number(flags.line);
  if (!Number.isInteger(lineNumber) || lineNumber < 1) fail("Usage: acp-trace.mjs show [file|dir] --line N [--full]");
  const event = events.find((candidate) => candidate.line === lineNumber);
  if (!event) fail(`Line ${lineNumber} not found in ${tracePath}`);
  const raw = parseRaw(event);
  console.log(JSON.stringify(flags.full ? { event, raw } : shorten({ event, raw }), null, 2));
}

function summarizeToolUpdate(line, update) {
  const raw = update.rawInput ?? update.rawOutput ?? {};
  return {
    line,
    phase: update.sessionUpdate,
    kind: update.kind,
    title: update.title,
    status: update.status,
    contentTypes: Array.isArray(update.content) ? update.content.map(contentType) : [],
    locationCount: Array.isArray(update.locations) ? update.locations.length : 0,
    rawInputKeys: update.rawInput ? Object.keys(update.rawInput).sort() : [],
    rawOutputKeys: update.rawOutput ? Object.keys(update.rawOutput).sort() : [],
    parsedCmdTypes: raw.parsed_cmd?.map((cmd) => cmd.type) ?? [],
    exitCode: update.rawOutput?.exit_code,
    stdoutBytes: update.rawOutput?.stdout ? Buffer.byteLength(update.rawOutput.stdout, "utf8") : 0,
    stderrBytes: update.rawOutput?.stderr ? Buffer.byteLength(update.rawOutput.stderr, "utf8") : 0,
  };
}

function preferredSessionUpdates(events) {
  const rawRows = [];
  const typedRows = [];
  for (const event of events) {
    const raw = parseRaw(event);
    if (raw?.method === "session/update") rawRows.push({ line: event.line, raw, event });
    if (event.event === "session/update") typedRows.push({ line: event.line, raw, event });
  }
  return rawRows.length ? rawRows : typedRows;
}

function preferredPermissionRequests(events) {
  const rawRows = [];
  const typedRows = [];
  for (const event of events) {
    const raw = parseRaw(event);
    if (raw?.method === "session/request_permission") rawRows.push({ line: event.line, raw, event });
    if (event.event === "session/request_permission.request") typedRows.push({ line: event.line, raw, event });
  }
  return rawRows.length ? rawRows : typedRows;
}

function readTrace(tracePath) {
  return readFileSync(tracePath, "utf8")
    .split(/\r?\n/)
    .map((text, index) => ({ text, line: index + 1 }))
    .filter(({ text }) => text.trim())
    .map(({ text, line }) => {
      try {
        return { ...JSON.parse(text), line };
      } catch (error) {
        return { event: "parse_error", line, error: error instanceof Error ? error.message : String(error) };
      }
    });
}

function parseRaw(event) {
  if (event.event !== "raw_line" || typeof event.payload?.line !== "string") return undefined;
  try {
    return JSON.parse(event.payload.line);
  } catch {
    return undefined;
  }
}

function resolveTraceFile(input) {
  const target = input ? resolve(input) : latestTraceFromDefaultDirs();
  if (!target) fail("No ACP trace found. Pass a trace file or directory.");
  if (!existsSync(target)) fail(`Path not found: ${target}`);
  const stat = statSync(target);
  if (stat.isFile()) return target;
  if (stat.isDirectory()) {
    const latest = latestTraceInDir(target);
    if (!latest) fail(`No *.jsonl traces found under: ${target}`);
    return latest;
  }
  fail(`Not a file or directory: ${target}`);
}

function latestTraceFromDefaultDirs() {
  const dirs = [
    process.env.OPENAIDE_ACP_TRACE_DIR,
    ...findTraceDirs(join(homedir(), ".vscode-server", "data", "User", "globalStorage"), 6),
    ...findTraceDirs(join(homedir(), ".config", "Code", "User", "globalStorage"), 6),
  ].filter(Boolean);
  const files = dirs.flatMap((dir) => traceFilesInDir(dir));
  return newest(files);
}

function findTraceDirs(root, depth) {
  if (!existsSync(root) || depth < 0) return [];
  const found = [];
  for (const entry of safeReaddir(root)) {
    const path = join(root, entry.name);
    if (!entry.isDirectory()) continue;
    if (entry.name === "acp-traces") {
      found.push(path);
      continue;
    }
    found.push(...findTraceDirs(path, depth - 1));
  }
  return found;
}

function latestTraceInDir(dir) {
  return newest(traceFilesInDir(dir));
}

function traceFilesInDir(dir) {
  if (!dir || !existsSync(dir)) return [];
  return safeReaddir(dir)
    .filter((entry) => entry.isFile() && entry.name.endsWith(".jsonl"))
    .map((entry) => join(dir, entry.name));
}

function newest(paths) {
  return paths
    .filter(Boolean)
    .sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs)[0];
}

function safeReaddir(path) {
  try {
    return readdirSync(path, { withFileTypes: true });
  } catch {
    return [];
  }
}

function parseTarget(values) {
  const flags = {};
  const positional = [];
  for (let index = 0; index < values.length; index += 1) {
    const value = values[index];
    if (value === "--full") {
      flags.full = true;
    } else if (value === "--line") {
      flags.line = values[++index];
    } else {
      positional.push(value);
    }
  }
  return { target: positional[0] ?? latestTraceFromDefaultDirs(), flags };
}

function contentType(value) {
  return value.type ?? value.content?.type ?? Object.keys(value).sort().join(",");
}

function increment(map, key) {
  map.set(key, (map.get(key) ?? 0) + 1);
}

function sortedEntries(map) {
  return [...map.entries()].sort((a, b) => b[1] - a[1]);
}

function unique(values) {
  return [...new Set(values.filter(Boolean))];
}

function shorten(value) {
  if (typeof value === "string") return value.length > 160 ? `<string:${value.length}>` : value;
  if (Array.isArray(value)) return value.slice(0, 8).map(shorten).concat(value.length > 8 ? [`<${value.length - 8} more>`] : []);
  if (value && typeof value === "object") {
    const output = {};
    for (const [key, child] of Object.entries(value)) output[key] = shorten(child);
    return output;
  }
  return value;
}

function fail(message) {
  console.error(message);
  process.exit(1);
}

main();
