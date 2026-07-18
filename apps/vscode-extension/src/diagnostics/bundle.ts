import { open } from "node:fs/promises";
import * as path from "node:path";
import type { DiagnosticsSnapshot } from "@openaide/app-shell-contracts";

const LOG_WINDOW_MS = 24 * 60 * 60 * 1_000;
const MAX_LOG_BYTES = 2 * 1024 * 1024;
const encoder = new TextEncoder();

export type SupportEnvironment = {
  platform: string;
  architecture: string;
  vscode_version: string;
  extension_version: string;
};

type LogSourceManifest = {
  status: "included" | "unavailable";
  record_count: number;
  discarded_record_count: number;
};

export type SupportBundleManifest = {
  schema_version: 1;
  created_at: string;
  log_window: {
    since: string;
    max_bytes_per_source: number;
  };
  sources: {
    extension_log: LogSourceManifest;
    app_server_log: LogSourceManifest;
  };
  excluded_data: string[];
};

export type SupportBundle = {
  bytes: Uint8Array;
  manifest: SupportBundleManifest;
};

/** Builds the only user-shareable diagnostics artifact from strict allowlists. */
export async function buildSupportBundle(input: {
  snapshot: DiagnosticsSnapshot;
  environment: SupportEnvironment;
  diagnosticsLogDirectory: string;
  now?: Date;
}): Promise<SupportBundle> {
  const now = input.now ?? new Date();
  const since = new Date(now.getTime() - LOG_WINDOW_MS);
  const extensionLog = await collectLog(
    path.join(input.diagnosticsLogDirectory, "openaide-extension.jsonl"),
    since,
  );
  const appServerLog = await collectLog(
    path.join(input.diagnosticsLogDirectory, "openaide-app-server.jsonl"),
    since,
  );
  const manifest: SupportBundleManifest = {
    schema_version: 1,
    created_at: now.toISOString(),
    log_window: {
      since: since.toISOString(),
      max_bytes_per_source: MAX_LOG_BYTES,
    },
    sources: {
      extension_log: extensionLog.manifest,
      app_server_log: appServerLog.manifest,
    },
    excluded_data: [
      "prompts_and_chat",
      "file_contents_and_paths",
      "terminal_output",
      "environment_variables_and_secrets",
      "raw_error_text",
      "raw_protocol_payloads",
    ],
  };
  const entries: Record<string, Uint8Array> = {
    "manifest.json": jsonFile(manifest),
    "snapshot.json": jsonFile(publicSnapshot(input.snapshot)),
    "environment.json": jsonFile(publicEnvironment(input.environment)),
  };
  if (extensionLog.content !== undefined) {
    entries["logs/openaide-extension.jsonl"] = encoder.encode(extensionLog.content);
  }
  if (appServerLog.content !== undefined) {
    entries["logs/openaide-app-server.jsonl"] = encoder.encode(appServerLog.content);
  }
  return { bytes: createStoredZip(entries), manifest };
}

async function collectLog(filePath: string, since: Date): Promise<{
  content?: string;
  manifest: LogSourceManifest;
}> {
  let file;
  try {
    file = await open(filePath, "r");
    const stat = await file.stat();
    const byteCount = Math.min(stat.size, MAX_LOG_BYTES);
    const buffer = Buffer.alloc(byteCount);
    await file.read(buffer, 0, byteCount, stat.size - byteCount);
    const tail = discardPartialFirstLine(buffer.toString("utf8"), stat.size > byteCount);
    let discarded = 0;
    const records: string[] = [];
    for (const line of tail.split("\n")) {
      if (!line.trim()) continue;
      const record = publicLogRecord(line, since);
      if (record) records.push(JSON.stringify(record));
      else discarded += 1;
    }
    const cappedRecords = newestRecordsWithinByteCap(records);
    discarded += records.length - cappedRecords.length;
    return {
      content: cappedRecords.length > 0 ? `${cappedRecords.join("\n")}\n` : "",
      manifest: {
        status: "included",
        record_count: cappedRecords.length,
        discarded_record_count: discarded,
      },
    };
  } catch {
    return {
      manifest: {
        status: "unavailable",
        record_count: 0,
        discarded_record_count: 0,
      },
    };
  } finally {
    await file?.close();
  }
}

function newestRecordsWithinByteCap(records: string[]) {
  let firstIncluded = records.length;
  let bytes = 0;
  while (firstIncluded > 0) {
    const nextBytes = Buffer.byteLength(records[firstIncluded - 1]!) + 1;
    if (bytes + nextBytes > MAX_LOG_BYTES) break;
    bytes += nextBytes;
    firstIncluded -= 1;
  }
  return records.slice(firstIncluded);
}

function publicLogRecord(line: string, since: Date) {
  let input: unknown;
  try {
    input = JSON.parse(line);
  } catch {
    return undefined;
  }
  if (!isRecord(input)) return undefined;
  const timestamp = logTimestamp(input);
  const scope = safeIdentifier(input.scope);
  const level = logLevel(input.level);
  const event = safeIdentifier(input.event);
  if (!timestamp || timestamp < since || !scope || !level || !event) return undefined;
  return {
    timestamp: timestamp.toISOString(),
    scope,
    level,
    event,
    fields: publicLogFields(input.fields),
    ...controlledFailure(event),
  };
}

function logTimestamp(input: Record<string, unknown>) {
  const value = typeof input.timestamp === "string"
    ? Date.parse(input.timestamp)
    : typeof input.timestamp_ms === "number"
      ? input.timestamp_ms
      : Number.NaN;
  if (!Number.isFinite(value)) return undefined;
  const timestamp = new Date(value);
  return Number.isFinite(timestamp.getTime()) ? timestamp : undefined;
}

function logLevel(value: unknown): "info" | "warn" | "error" | undefined {
  return value === "info" || value === "warn" || value === "error" ? value : undefined;
}

function publicLogFields(value: unknown) {
  if (!isRecord(value)) return {};
  const output: Record<string, boolean | number | string | null> = {};
  for (const [key, fieldValue] of Object.entries(value)) {
    if (!isSafeFieldName(key)) continue;
    if (fieldValue === null || typeof fieldValue === "boolean" || typeof fieldValue === "number") {
      output[key] = fieldValue;
      continue;
    }
    if (typeof fieldValue !== "string") continue;
    const safeValue = key === "method" ? safeMethod(fieldValue) : safeMetadataToken(fieldValue);
    if (safeValue) output[key] = safeValue;
  }
  return output;
}

function isSafeFieldName(key: string) {
  return ["method", "signal", "code", "idType", "commandKind", "storageRootKind", "byteLength"].includes(key)
    || /_(kind|code|count|bytes|status)$/.test(key);
}

function controlledFailure(event: string) {
  switch (event) {
    case "app_server_spawn_failed":
      return { error_kind: "process_spawn_failed", summary: "The App Server process could not start." };
    case "rpc_request_failed":
      return { error_kind: "request_failed", summary: "An App Server request failed." };
    default:
      return {};
  }
}

function publicSnapshot(snapshot: DiagnosticsSnapshot): DiagnosticsSnapshot {
  // Runtime diagnostics are already allowlisted; reconstruct process state so
  // implementation-only fields such as the local log directory cannot leak.
  const agentAliases = new Map<string, string>();
  return {
    created_at: snapshot.created_at,
    runtime: {
      ...snapshot.runtime,
      tasks: {
        ...snapshot.runtime.tasks,
        active_tasks: snapshot.runtime.tasks.active_tasks.map((task) => ({
          ...task,
          agent_id: publicAgentId(task.agent_id, agentAliases),
        })),
      },
    },
    notices: snapshot.notices.map((notice) => ({
      component: notice.component,
      severity: notice.severity,
      message: notice.severity === "error" ? "Runtime diagnostics were unavailable." : "Diagnostic notice recorded.",
    })),
    process: {
      running: snapshot.process.running,
      runtime_source_kind: snapshot.process.runtime_source_kind,
      storage_root_kind: snapshot.process.storage_root_kind,
    },
  };
}

function publicAgentId(agentId: string, aliases: Map<string, string>) {
  if (agentId === "codex" || agentId === "opencode") return agentId;
  const existing = aliases.get(agentId);
  if (existing) return existing;
  const alias = `agent:${aliases.size + 1}`;
  aliases.set(agentId, alias);
  return alias;
}

function publicEnvironment(environment: SupportEnvironment): SupportEnvironment {
  return {
    platform: safeIdentifier(environment.platform) ?? "unknown",
    architecture: safeIdentifier(environment.architecture) ?? "unknown",
    vscode_version: safeIdentifier(environment.vscode_version) ?? "unknown",
    extension_version: safeIdentifier(environment.extension_version) ?? "unknown",
  };
}

function safeIdentifier(value: unknown) {
  return typeof value === "string" && /^[A-Za-z0-9_.:-]{1,128}$/.test(value) ? value : undefined;
}

function safeMethod(value: string) {
  return /^[A-Za-z][A-Za-z0-9_.-]*(?:\/[A-Za-z0-9_.-]+)*$/.test(value) ? value : undefined;
}

function safeMetadataToken(value: string) {
  return /^[A-Za-z0-9_-]{1,128}$/.test(value) ? value : undefined;
}

function discardPartialFirstLine(text: string, startsMidFile: boolean) {
  if (!startsMidFile) return text;
  const firstNewline = text.indexOf("\n");
  return firstNewline === -1 ? "" : text.slice(firstNewline + 1);
}

function jsonFile(value: unknown) {
  return encoder.encode(`${JSON.stringify(value, null, 2)}\n`);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** Writes a portable ZIP using the store method; support bundles are already size-capped. */
function createStoredZip(entries: Record<string, Uint8Array>) {
  const localParts: Buffer[] = [];
  const centralParts: Buffer[] = [];
  let localOffset = 0;
  for (const [name, content] of Object.entries(entries)) {
    const nameBytes = Buffer.from(name, "utf8");
    const data = Buffer.from(content);
    const checksum = crc32(data);
    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4);
    local.writeUInt16LE(0x21, 12);
    local.writeUInt32LE(checksum, 14);
    local.writeUInt32LE(data.length, 18);
    local.writeUInt32LE(data.length, 22);
    local.writeUInt16LE(nameBytes.length, 26);
    localParts.push(local, nameBytes, data);

    const central = Buffer.alloc(46);
    central.writeUInt32LE(0x02014b50, 0);
    central.writeUInt16LE(20, 4);
    central.writeUInt16LE(20, 6);
    central.writeUInt16LE(0x21, 14);
    central.writeUInt32LE(checksum, 16);
    central.writeUInt32LE(data.length, 20);
    central.writeUInt32LE(data.length, 24);
    central.writeUInt16LE(nameBytes.length, 28);
    central.writeUInt32LE(localOffset, 42);
    centralParts.push(central, nameBytes);
    localOffset += local.length + nameBytes.length + data.length;
  }
  const centralDirectory = Buffer.concat(centralParts);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(Object.keys(entries).length, 8);
  end.writeUInt16LE(Object.keys(entries).length, 10);
  end.writeUInt32LE(centralDirectory.length, 12);
  end.writeUInt32LE(localOffset, 16);
  return Buffer.concat([...localParts, centralDirectory, end]);
}

function crc32(data: Uint8Array) {
  let crc = 0xffffffff;
  for (const byte of data) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1) {
      crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
    }
  }
  return (crc ^ 0xffffffff) >>> 0;
}
