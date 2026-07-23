#!/usr/bin/env node
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { readAgentSettingsRecords } from "./agent-settings.mjs";
import { readSplitProjectionMaybe, splitMetadata } from "./split-task-store.mjs";

const HOME = os.homedir();
const DEFAULT_LIMIT = 50;

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
});

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const command = args.positionals[0] ?? "help";
  if (command === "help" || args.flags.help) return printHelp();

  const roots = findRoots(args.flags.root);
  if (command === "roots") return printRoots(roots, args.flags.json);
  if (command === "agents") return printAgentSettings(args);
  if (roots.length === 0) {
    throw new Error("No OpenAIDE App Server artifact roots found. Pass --root <extension-storage-or-runtime-dir>.");
  }

  if (command === "logs") return printLogs(roots, args);
  if (command === "failures") return printFailures(roots, args);
  if (command === "tasks") return printTasks(roots, args);
  if (command === "task") return printTask(roots, args);
  if (command === "messages") return printMessages(roots, args);
  if (command === "search") return searchArtifacts(roots, args);
  if (command === "doctor") return doctor(roots, args);
  if (command === "export") return exportReport(roots, args);

  throw new Error(`Unknown command: ${command}`);
}

function parseArgs(argv) {
  const flags = {};
  const positionals = [];
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (!arg.startsWith("--")) {
      positionals.push(arg);
      continue;
    }
    const key = arg.slice(2);
    if (["json", "full", "all-roots", "help", "redact"].includes(key)) {
      flags[key] = true;
      continue;
    }
    const value = argv[index + 1];
    if (value === undefined || value.startsWith("--")) {
      throw new Error(`Missing value for --${key}`);
    }
    flags[key] = value;
    index += 1;
  }
  return { flags, positionals };
}

function findRoots(explicitRoot) {
  const candidates = [];
  if (explicitRoot) return normalizeCandidateRoots([expandHome(explicitRoot)]);
  if (process.env.OPENAIDE_ARTIFACT_ROOT) candidates.push(expandHome(process.env.OPENAIDE_ARTIFACT_ROOT));

  const vscodeGlobalStorageGlobs = [
    path.join(HOME, ".vscode-server", "data", "User", "globalStorage"),
    path.join(HOME, ".vscode-server-insiders", "data", "User", "globalStorage"),
    path.join(HOME, ".config", "Code", "User", "globalStorage"),
    path.join(HOME, ".config", "Code - Insiders", "User", "globalStorage"),
  ];
  for (const globalStorage of vscodeGlobalStorageGlobs) {
    if (!fs.existsSync(globalStorage)) continue;
    for (const entry of fs.readdirSync(globalStorage)) {
      if (entry.toLowerCase().includes("openaide")) {
        candidates.push(path.join(globalStorage, entry));
      }
    }
  }
  candidates.push(...localWebStateCandidates(process.cwd()));

  return normalizeCandidateRoots(candidates);
}

function normalizeCandidateRoots(candidates) {
  const roots = [];
  for (const candidate of candidates) {
    const root = normalizeRuntimeRoot(candidate);
    if (!root) continue;
    if (!roots.some((item) => item.runtimeRoot === root.runtimeRoot)) roots.push(root);
  }
  return roots.sort((left, right) => left.runtimeRoot.localeCompare(right.runtimeRoot));
}

function localWebStateCandidates(startDir) {
  for (const dir of ancestorDirs(startDir)) {
    const candidates = [];
    for (const entry of readdirMaybe(dir)) {
      if (!entry.startsWith(".openaide-web-")) continue;
      candidates.push(path.join(dir, entry));
      candidates.push(path.join(dir, entry, "state"));
    }
    // A local instance belongs to its nearest enclosing project. Looking above
    // that boundary can accidentally mix unrelated repositories or /tmp runs.
    if (candidates.length > 0) return candidates;
  }
  return [];
}

function ancestorDirs(startDir) {
  const dirs = [];
  let current = path.resolve(startDir);
  while (true) {
    dirs.push(current);
    const parent = path.dirname(current);
    if (parent === current) return dirs;
    current = parent;
  }
}

function normalizeRuntimeRoot(candidate) {
  const stat = statMaybe(candidate);
  if (!stat?.isDirectory()) return undefined;
  const splitTasksRoot = path.join(candidate, "task-store-v1", "tasks");
  if (fs.existsSync(splitTasksRoot)) {
    return { storageRoot: candidate, runtimeRoot: candidate, tasksRoot: splitTasksRoot, layout: "split-v1" };
  }
  if (fs.existsSync(path.join(candidate, "tasks"))) {
    return {
      storageRoot: path.dirname(candidate),
      runtimeRoot: candidate,
      tasksRoot: path.join(candidate, "tasks"),
      layout: "legacy",
    };
  }
  const runtimeRoot = path.join(candidate, "runtime");
  const runtimeSplitTasksRoot = path.join(runtimeRoot, "task-store-v1", "tasks");
  if (fs.existsSync(runtimeSplitTasksRoot)) {
    return {
      storageRoot: candidate,
      runtimeRoot,
      tasksRoot: runtimeSplitTasksRoot,
      layout: "split-v1",
    };
  }
  if (fs.existsSync(path.join(runtimeRoot, "tasks"))) {
    return {
      storageRoot: candidate,
      runtimeRoot,
      tasksRoot: path.join(runtimeRoot, "tasks"),
      layout: "legacy",
    };
  }
  return undefined;
}

function printRoots(roots, asJson) {
  if (asJson) return console.log(JSON.stringify({ roots }, null, 2));
  if (roots.length === 0) {
    console.log("No OpenAIDE artifact roots found.");
    return;
  }
  for (const root of roots) {
    console.log(`${root.runtimeRoot} (${readTasks(root).length} tasks)`);
  }
}

function printTasks(roots, args) {
  const tasks = allTasks(roots);
  const filtered = tasks
    .filter((entry) => !args.flags.status || entry.task?.status === args.flags.status)
    .filter((entry) => !args.flags.query || haystack(entry).toLowerCase().includes(String(args.flags.query).toLowerCase()))
    .sort((left, right) => String(right.task?.last_activity ?? "").localeCompare(String(left.task?.last_activity ?? "")))
    .slice(0, numberFlag(args.flags.limit, DEFAULT_LIMIT));
  if (args.flags.json) return console.log(JSON.stringify({ tasks: filtered }, null, 2));
  if (filtered.length === 0) return console.log("No tasks.");
  for (const entry of filtered) {
    const task = entry.task ?? {};
    const messageSummary = entry.storage.layout === "split-v1" ? "messages=lazy" : `messages=${entry.messageCount ?? "?"}`;
    console.log(
      [
        task.task_id ?? entry.taskId,
        task.status ?? "?",
        task.unread ? "unread" : "read",
        taskTitle(task),
        messageSummary,
        shortPath(entry.root.runtimeRoot),
      ].join(" | "),
    );
  }
}

function printLogs(roots, args) {
  const entries = readLogEntries(roots, args.positionals[1] ?? "all")
    .filter((entry) => !args.flags.level || entry.level === args.flags.level)
    .filter((entry) => !args.flags.grep || JSON.stringify(entry).toLowerCase().includes(String(args.flags.grep).toLowerCase()))
    .sort((left, right) => logTime(left) - logTime(right));
  const selected = entries.slice(-numberFlag(args.flags.limit, DEFAULT_LIMIT));
  if (args.flags.json) return console.log(JSON.stringify({ entries: selected.map(redactLogEntry) }, null, 2));
  for (const entry of selected) {
    console.log(formatLogEntry(entry));
  }
}

function printFailures(roots, args) {
  const entries = readLogEntries(roots, args.positionals[1] ?? "all")
    .filter(isFailureLog)
    .filter((entry) => !args.flags.grep || JSON.stringify(entry).toLowerCase().includes(String(args.flags.grep).toLowerCase()))
    .sort((left, right) => logTime(left) - logTime(right));
  const selected = entries.slice(-numberFlag(args.flags.limit, DEFAULT_LIMIT));
  if (args.flags.json) return console.log(JSON.stringify({ entries: selected.map(redactLogEntry) }, null, 2));
  if (selected.length === 0) return console.log("No failures.");
  for (const entry of selected) {
    console.log(formatLogEntry(entry));
  }
}

function printAgentSettings(args) {
  const records = readAgentSettingsRecords()
    .filter((record) => !args.flags.grep || JSON.stringify(record).toLowerCase().includes(String(args.flags.grep).toLowerCase()))
    .sort((left, right) => String(left.file).localeCompare(String(right.file)));
  if (args.flags.json) return console.log(JSON.stringify({ records: redactObject(records) }, null, 2));
  if (records.length === 0) {
    console.log("No openaide.agents settings found.");
    return;
  }
  for (const record of records) {
    console.log(`${shortPath(record.file)}${record.history ? " [history]" : ""}`);
    for (const agent of record.agents) {
      console.log(
        indent(
          [
            agent.id ?? "(no id)",
            agent.label ?? "(no label)",
            agent.enabled === false ? "disabled" : "enabled",
            agent.icon ? `icon=${agent.icon}` : undefined,
            agent.transport || agent.launch?.transport ? `transport=${agent.transport ?? agent.launch?.transport}` : undefined,
            agent.command_line || agent.launch?.command_line ? `command=${agent.command_line ?? agent.launch?.command_line}` : undefined,
          ]
            .filter(Boolean)
            .join(" | "),
          "  ",
        ),
      );
    }
  }
}

function printTask(roots, args) {
  const entry = requireTask(roots, args.positionals[1]);
  if (args.flags.json) return console.log(JSON.stringify(entry, null, 2));
  console.log(JSON.stringify(entry.task, null, 2));
  console.log(`messages: ${entry.messageCount ?? "?"}`);
  console.log(`root: ${entry.root.runtimeRoot}`);
}

function printMessages(roots, args) {
  const entry = requireTask(roots, args.positionals[1]);
  const messages = readMessages(entry);
  const limit = args.flags.full ? messages.length : numberFlag(args.flags.limit, DEFAULT_LIMIT);
  const selected = messages.slice(Math.max(0, messages.length - limit));
  if (args.flags.json) return console.log(JSON.stringify({ task: entry.task, messages: selected }, null, 2));
  for (const stored of selected) {
    const chat = stored.chat ?? {};
    const message = chat.message ?? {};
    console.log(`${stored.sequence ?? "?"} ${chat.message_type ?? message.kind ?? "?"} ${chat.identity ?? ""}`);
    console.log(indent(previewMessage(message, args.flags.full), "  "));
  }
}

function searchArtifacts(roots, args) {
  const query = args.positionals.slice(1).join(" ");
  if (!query) throw new Error("Usage: search <text>");
  const lower = query.toLowerCase();
  const limit = numberFlag(args.flags.limit, DEFAULT_LIMIT);
  const hits = [];
  for (const entry of allTasks(roots)) {
    const split = entry.storage.layout === "split-v1";
    for (const file of split ? ["task.json"] : ["task.json", "message_meta.json", "messages.jsonl"]) {
      const filePath = path.join(entry.taskDir, file);
      if (!fs.existsSync(filePath)) continue;
      const lines = fs.readFileSync(filePath, "utf8").split(/\n/);
      for (let index = 0; index < lines.length; index += 1) {
        const line = lines[index];
        if (!line.toLowerCase().includes(lower)) continue;
        hits.push({
          task_id: entry.taskId,
          title: entry.task?.title,
          file: filePath,
          line: index + 1,
          preview: redact(line.slice(0, 220)),
        });
        if (hits.length >= limit) break;
      }
      if (hits.length >= limit) break;
    }
    if (split && hits.length < limit) {
      for (const stored of readMessages(entry)) {
        const serialized = JSON.stringify(stored);
        if (!serialized.toLowerCase().includes(lower)) continue;
        hits.push({
          task_id: entry.taskId,
          title: taskTitle(entry.task),
          source: "materialized-chat",
          sequence: stored.sequence,
          preview: redact(serialized.slice(0, 220)),
        });
        if (hits.length >= limit) break;
      }
    }
    if (hits.length >= limit) break;
  }
  if (args.flags.json) return console.log(JSON.stringify({ query, hits }, null, 2));
  for (const hit of hits) {
    const location = hit.source === "materialized-chat" ? `chat:${hit.sequence}` : `${path.basename(hit.file)}:${hit.line}`;
    console.log(`${hit.task_id} ${location} ${hit.preview}`);
  }
}

function readLogEntries(roots, target) {
  return roots.flatMap((root) =>
    logFiles(root, target).flatMap((file) =>
      readJsonl(file).map((entry) => ({
        ...entry,
        component: componentFromLogFile(file),
        file,
      })),
    ),
  );
}

function logFiles(root, target) {
  const logRoot = path.join(root.runtimeRoot, "diagnostics", "logs");
  const files = [];
  const normalized = String(target).toLowerCase();
  if (normalized === "all" || normalized === "extension") files.push(path.join(logRoot, "openaide-extension.jsonl"));
  if (normalized === "all" || normalized === "runtime") {
    files.push(path.join(logRoot, "openaide-app-server.jsonl"));
    files.push(path.join(logRoot, "openaide-runtime.jsonl"));
  }
  if (!["all", "extension", "runtime"].includes(normalized)) {
    throw new Error("Usage: logs [all|extension|runtime] [--limit 50] [--level warn] [--grep text] [--json]");
  }
  return files.filter((file) => fs.existsSync(file));
}

function readJsonl(file) {
  try {
    return fs
      .readFileSync(file, "utf8")
      .split(/\n/)
      .filter((line) => line.trim())
      .map((line) => JSON.parse(line));
  } catch (error) {
    return [
      {
        level: "error",
        message: `Invalid JSONL: ${error.message}`,
        file,
      },
    ];
  }
}

function componentFromLogFile(file) {
  const name = path.basename(file);
  if (name.includes("extension")) return "extension";
  if (name.includes("runtime") || name.includes("app-server")) return "runtime";
  return "unknown";
}

function logTime(entry) {
  if (typeof entry.timestamp_ms === "number") return entry.timestamp_ms;
  const parsed = Date.parse(entry.timestamp ?? "");
  return Number.isNaN(parsed) ? 0 : parsed;
}

function isFailureLog(entry) {
  if (entry.level === "warn" || entry.level === "error") return true;
  const text = JSON.stringify(entry).toLowerCase();
  return text.includes("failed") || text.includes("error") || text.includes("timed out") || text.includes("not found");
}

function redactLogEntry(entry) {
  return redactObject(entry);
}

function formatLogEntry(entry) {
  const time = entry.timestamp ?? (entry.timestamp_ms ? new Date(entry.timestamp_ms).toISOString() : "?");
  const title = entry.message ?? entry.event ?? "?";
  const fields = entry.fields ?? {};
  const detail = fields.error ?? fields.type ?? fields.method ?? "";
  return redact([time, entry.component ?? "?", entry.level ?? "?", title, detail].filter(Boolean).join(" | "));
}

function doctor(roots, args) {
  const taskId = args.positionals[1];
  const entries = taskId ? [requireTask(roots, taskId)] : allTasks(roots);
  const findings = [];
  for (const entry of entries) {
    findings.push(...doctorTask(entry));
  }
  const summary = {
    task_count: entries.length,
    finding_count: findings.length,
    findings,
  };
  if (args.flags.json) return console.log(JSON.stringify(summary, null, 2));
  if (findings.length === 0) {
    console.log(`No findings across ${entries.length} task(s).`);
    return;
  }
  for (const finding of findings) {
    console.log(`${finding.severity.toUpperCase()} ${finding.task_id} ${finding.code}: ${finding.message}`);
  }
}

function exportReport(roots, args) {
  const taskId = args.positionals[1];
  const outIndex = process.argv.indexOf("--out");
  const out = outIndex >= 0 ? process.argv[outIndex + 1] : undefined;
  if (!taskId || !out) throw new Error("Usage: export <task-id-or-prefix> --out <file>");
  const entry = requireTask(roots, taskId);
  const projection = readProjection(entry);
  const report = {
    generated_at: new Date().toISOString(),
    root: args.flags.redact === false ? entry.root.runtimeRoot : redact(entry.root.runtimeRoot),
    task: entry.task,
    message_meta: projection.messageMeta,
    findings: doctorTask(entry),
    messages: projection.messages,
  };
  const text = JSON.stringify(args.flags.redact === false ? report : redactObject(report), null, 2);
  fs.mkdirSync(path.dirname(path.resolve(out)), { recursive: true });
  fs.writeFileSync(out, `${text}\n`);
  console.log(out);
}

function doctorTask(entry) {
  const findings = [];
  const task = entry.task;
  const taskId = entry.taskId;
  const taskJson = readJsonMaybe(path.join(entry.taskDir, "task.json"));
  const split = splitMetadata(taskJson.value);
  const metaJson = split
    ? readSplitProjectionMaybe(entry.taskDir, split)
    : readJsonMaybe(path.join(entry.taskDir, "message_meta.json"));
  const messagesResult = split ? metaJson : readMessagesMaybe(entry);
  if (taskJson.error) findings.push(finding("error", taskId, "task-json-invalid", taskJson.error));
  if (metaJson.error) findings.push(finding("error", taskId, "chat-storage-invalid", metaJson.error));
  if (!split && messagesResult.error) findings.push(finding("error", taskId, "messages-jsonl-invalid", messagesResult.error));
  const messages = split ? (messagesResult.value?.messages ?? []) : (messagesResult.value ?? []);
  const messageMeta = split ? metaJson.value?.messageMeta : metaJson.value;
  if (split && metaJson.value?.journal.incompleteTailBytes > 0) {
    findings.push(
      finding(
        "warning",
        taskId,
        "incomplete-chat-tail",
        `Chat journal has ${metaJson.value.journal.incompleteTailBytes} recoverable trailing byte(s).`,
      ),
    );
  }

  if (!taskJson.value) findings.push(finding("error", taskId, "task-json-missing", "Missing task.json."));
  if (!messageMeta) findings.push(finding("warning", taskId, "message-meta-missing", "Missing persisted Chat metadata."));

  if (messageMeta && messages.length !== messageMeta.message_count) {
    findings.push(
      finding("error", taskId, "message-count-mismatch", `message metadata count=${messageMeta.message_count}, actual=${messages.length}.`),
    );
  }
  if (task && messageMeta && task.message_history_version !== messageMeta.version) {
    findings.push(
      finding("warning", taskId, "history-version-mismatch", `task message_history_version=${task.message_history_version}, meta version=${messageMeta.version}.`),
    );
  }

  const identities = new Map();
  for (const stored of messages) {
    const identity = stored.chat?.identity;
    if (!identity) continue;
    identities.set(identity, (identities.get(identity) ?? 0) + 1);
  }
  for (const [identity, count] of identities) {
    if (count > 1) findings.push(finding("error", taskId, "duplicate-identity", `${identity} appears ${count} times.`));
  }

  for (const field of ["created_at", "updated_at", "last_activity"]) {
    if (task?.[field] && !isParseableTime(task[field])) {
      findings.push(finding("warning", taskId, "task-time-unparseable", `${field} is not parseable by Date.parse: ${task[field]}`));
    }
  }
  const lastActivityMs = parseTimeMs(task?.last_activity);
  if (task?.status === "active" && task.active_turn_id && lastActivityMs !== undefined) {
    const ageMs = Date.now() - lastActivityMs;
    const staleActiveTurnMs = 10 * 60 * 1000;
    if (ageMs > staleActiveTurnMs) {
      findings.push(
        finding(
          "error",
          taskId,
          "stale-active-turn",
          `Task is active with ${task.active_turn_id}, but last activity was ${formatDuration(ageMs)} ago.`,
        ),
      );
    }
  }

  let adjacentText = [];
  for (const stored of [...messages, { chat: { message_type: "__sentinel" } }]) {
    const message = stored.chat?.message;
    if (message?.kind === "agent_text") {
      adjacentText.push(message);
      continue;
    }
    if (adjacentText.length >= 3) {
      const tiny = adjacentText.filter((item) => String(item.text ?? "").trim().length <= 8).length;
      if (tiny >= 2) {
        findings.push(
          finding("error", taskId, "fragmented-agent-text", `${adjacentText.length} adjacent agent_text chunks, ${tiny} tiny chunks.`),
        );
      }
    }
    adjacentText = [];
  }

  for (const stored of messages) {
    const message = stored.chat?.message;
    if (message?.kind !== "activity") continue;
    if (message.title === "Working" && message.steps?.some((step) => step.text === "Started")) {
      findings.push(finding("warning", taskId, "working-boilerplate", "Persisted Working/Started activity row."));
    }
    for (const step of message.steps ?? []) {
      if (step.kind === "tool" && !step.input_summary && !step.output_preview) {
        findings.push(finding("warning", taskId, "tool-without-preview", `Tool activity "${message.title}" has no input or output preview.`));
      }
    }
  }

  return findings;
}

function allTasks(roots) {
  return roots.flatMap((root) => readTasks(root).map((entry) => ({ ...entry, root })));
}

function readTasks(root) {
  if (!fs.existsSync(root.tasksRoot)) return [];
  return fs
    .readdirSync(root.tasksRoot)
    .filter((name) => name.startsWith("task_"))
    .map((taskId) => {
      const taskDir = path.join(root.tasksRoot, taskId);
      const taskFile = readJsonMaybe(path.join(taskDir, "task.json")).value;
      const split = splitMetadata(taskFile);
      const task = split?.task ?? taskFile;
      const meta = split ? undefined : readJsonMaybe(path.join(taskDir, "message_meta.json")).value;
      return {
        taskId,
        taskDir,
        task,
        messageCount: meta?.message_count,
        storage: split
          ? {
              layout: "split-v1",
              storageSequence: split.storageSequence,
              chatSequence: split.chatSequence,
              chatSnapshot: split.chatSnapshot,
              chatJournal: split.chatJournal,
            }
          : { layout: "legacy" },
      };
    });
}

function requireTask(roots, idOrPrefix) {
  if (!idOrPrefix) throw new Error("Task id or prefix required.");
  const matches = allTasks(roots).filter((entry) => entry.taskId === idOrPrefix || entry.taskId.startsWith(idOrPrefix));
  if (matches.length === 0) throw new Error(`No task found for ${idOrPrefix}`);
  if (matches.length > 1) {
    throw new Error(`Ambiguous task prefix ${idOrPrefix}: ${matches.map((entry) => entry.taskId).join(", ")}`);
  }
  return matches[0];
}

function readMessages(entry) {
  return readProjection(entry).messages;
}

function readProjection(entry) {
  const taskFile = readJsonMaybe(path.join(entry.taskDir, "task.json"));
  if (taskFile.error) throw new Error(taskFile.error);
  const split = splitMetadata(taskFile.value);
  if (!split) {
    const result = readMessagesMaybe(entry);
    if (result.error) throw new Error(result.error);
    return {
      messages: result.value ?? [],
      messageMeta: readJsonMaybe(path.join(entry.taskDir, "message_meta.json")).value,
    };
  }
  const result = readSplitProjectionMaybe(entry.taskDir, split);
  if (result.error) throw new Error(result.error);
  return result.value;
}

function readMessagesMaybe(entry) {
  const file = path.join(entry.taskDir, "messages.jsonl");
  if (!fs.existsSync(file)) return { value: [] };
  try {
    const value = fs
      .readFileSync(file, "utf8")
      .split(/\n/)
      .filter((line) => line.trim())
      .map((line) => JSON.parse(line));
    return { value };
  } catch (error) {
    return { error: `${file}: ${error.message}` };
  }
}

function readJsonMaybe(file) {
  if (!fs.existsSync(file)) return { value: undefined };
  try {
    return { value: JSON.parse(fs.readFileSync(file, "utf8")) };
  } catch (error) {
    return { error: `${file}: ${error.message}` };
  }
}

function previewMessage(message, full) {
  if (full) return JSON.stringify(message, null, 2);
  if (message.kind === "agent_message") {
    return redact(
      (message.parts ?? [])
        .filter((part) => part.kind === "text")
        .map((part) => part.text)
        .join("")
        .slice(0, 240),
    );
  }
  if (message.kind === "user" || message.kind === "agent_text" || message.kind === "interruption") {
    return redact(String(message.text ?? message.message ?? "").slice(0, 240));
  }
  if (message.kind === "activity") {
    return `${message.title} [${message.status}] ${JSON.stringify(message.steps ?? []).slice(0, 240)}`;
  }
  if (message.kind === "permission") {
    return `${message.title} [${message.state}] ${message.tool_call?.title ?? ""}`;
  }
  return redact(JSON.stringify(message).slice(0, 240));
}

function haystack(entry) {
  return JSON.stringify([entry.taskId, entry.task, entry.messageCount]);
}

function taskTitle(task) {
  const title = task?.title;
  if (typeof title === "string") return title;
  if (title && typeof title.value === "string") return title.value;
  return "(untitled)";
}

function finding(severity, taskId, code, message) {
  return { severity, task_id: taskId, code, message };
}

function isParseableTime(value) {
  const text = String(value);
  return !Number.isNaN(Date.parse(text)) || /^\d{10,13}$/.test(text);
}

function parseTimeMs(value) {
  if (value === undefined || value === null || value === "") return undefined;
  const text = String(value);
  if (/^\d{10,13}$/.test(text)) {
    const parsed = Number(text);
    return text.length === 10 ? parsed * 1000 : parsed;
  }
  const parsed = Date.parse(text);
  return Number.isNaN(parsed) ? undefined : parsed;
}

function formatDuration(ms) {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  const remainingMinutes = minutes % 60;
  if (hours < 48) return `${hours}h ${remainingMinutes}m`;
  const days = Math.floor(hours / 24);
  return `${days}d ${hours % 24}h`;
}

function numberFlag(value, fallback) {
  if (value === undefined) return fallback;
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed < 1) throw new Error(`Invalid number: ${value}`);
  return Math.floor(parsed);
}

function expandHome(value) {
  if (value === "~") return HOME;
  if (value.startsWith("~/")) return path.join(HOME, value.slice(2));
  return path.resolve(value);
}

function statMaybe(file) {
  try {
    return fs.statSync(file);
  } catch {
    return undefined;
  }
}

function readdirMaybe(dir) {
  try {
    return fs.readdirSync(dir);
  } catch {
    return [];
  }
}

function shortPath(file) {
  return file.replace(HOME, "~");
}

function indent(text, prefix) {
  return String(text)
    .split(/\n/)
    .map((line) => `${prefix}${line}`)
    .join("\n");
}

function redact(value) {
  return String(value).replaceAll(HOME, "~");
}

function redactObject(value) {
  return JSON.parse(redact(JSON.stringify(value)));
}

function printHelp() {
  console.log(`OpenAIDE artifact diagnostics

Usage:
  oa-artifacts roots [--json]
  oa-artifacts logs [all|extension|runtime] [--level warn] [--grep text] [--limit 50] [--json]
  oa-artifacts failures [all|extension|runtime] [--grep text] [--limit 50] [--json]
  oa-artifacts agents [--grep text] [--json]
  oa-artifacts tasks [--status inactive] [--query text] [--limit 50] [--json]
  oa-artifacts task <task-id-or-prefix> [--json]
  oa-artifacts messages <task-id-or-prefix> [--limit 50] [--full] [--json]
  oa-artifacts search <text> [--limit 50] [--json]
  oa-artifacts doctor [task-id-or-prefix] [--json]
  oa-artifacts export <task-id-or-prefix> --out <file>

Options:
  --root <path>  State directory, extension storage directory, or legacy runtime directory.
  --json         Machine-readable output.
`);
}
