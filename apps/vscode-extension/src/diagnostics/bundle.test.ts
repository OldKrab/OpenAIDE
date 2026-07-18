import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import * as path from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import type { DiagnosticsSnapshot } from "@openaide/app-shell-contracts";
import { buildSupportBundle } from "./bundle";

const createdDirectories: string[] = [];

describe("support diagnostics bundle", () => {
  afterEach(async () => {
    await Promise.all(createdDirectories.splice(0).map((directory) => rm(directory, { recursive: true, force: true })));
  });

  it("packages recent public-safe logs with the runtime snapshot", async () => {
    const logDirectory = await temporaryDirectory();
    await writeFile(path.join(logDirectory, "openaide-extension.jsonl"), [
      JSON.stringify({
        timestamp: "2026-07-16T10:00:00.000Z",
        scope: "openaide-extension",
        level: "info",
        event: "old_event",
        fields: {},
      }),
      JSON.stringify({
        timestamp: "2026-07-18T09:45:00.000Z",
        scope: "openaide-extension",
        level: "warn",
        event: "app_server_spawn_failed",
        fields: {
          commandKind: "bundled",
          error_kind: "customer.private.example",
          error: "spawn /users/private/project token-secret",
          output: "private terminal output",
        },
      }),
    ].join("\n"));
    await writeFile(path.join(logDirectory, "openaide-app-server.jsonl"), `${JSON.stringify({
      timestamp_ms: Date.parse("2026-07-18T09:50:00.000Z"),
      scope: "openaide-app-server",
      level: "warn",
      event: "rpc_request_failed",
      fields: {
        method: "task/open",
        error: "private provider response",
      },
    })}\n`);

    const result = await buildSupportBundle({
      snapshot: snapshot(),
      environment: {
        platform: "linux",
        architecture: "x64",
        vscode_version: "1.100.0",
        extension_version: "0.0.1-alpha.4",
      },
      diagnosticsLogDirectory: logDirectory,
      now: new Date("2026-07-18T10:00:00.000Z"),
    });
    const entries = readStoredZip(result.bytes);
    const archive = Buffer.from(result.bytes);

    expect(archive.readUInt16LE(12)).toBe(0x21);
    expect([...entries.keys()].sort()).toEqual([
      "environment.json",
      "logs/openaide-app-server.jsonl",
      "logs/openaide-extension.jsonl",
      "manifest.json",
      "snapshot.json",
    ]);
    expect(entries.get("logs/openaide-extension.jsonl")).toContain("app_server_spawn_failed");
    expect(entries.get("logs/openaide-extension.jsonl")).not.toContain("old_event");
    expect(entries.get("logs/openaide-app-server.jsonl")).toContain("rpc_request_failed");
    expect(entries.get("logs/openaide-app-server.jsonl")).toContain("task/open");
    expect(archive.toString("utf8")).not.toContain("private");
    expect(archive.toString("utf8")).not.toContain("token-secret");
    expect(archive.toString("utf8")).not.toContain("customer.private.example");
    expect(JSON.parse(entries.get("manifest.json")!)).toMatchObject({
      schema_version: 1,
      created_at: "2026-07-18T10:00:00.000Z",
      log_window: {
        since: "2026-07-17T10:00:00.000Z",
        max_bytes_per_source: 2 * 1024 * 1024,
      },
      sources: {
        extension_log: { status: "included", record_count: 1 },
        app_server_log: { status: "included", record_count: 1 },
      },
    });
  });

  it("removes shell paths and tokenizes custom Agent identifiers", async () => {
    const logDirectory = await temporaryDirectory();
    const input = snapshot();
    input.runtime.tasks.active_tasks = [{
      task_id: "task-opaque-1",
      agent_id: "alice-private-agent-name",
      status: "running",
      updated_at: "2026-07-18T09:59:00.000Z",
      last_activity: "2026-07-18T09:59:00.000Z",
      has_agent_session: true,
    }];
    const snapshotWithPrivateHostField = {
      ...input,
      process: {
        ...input.process,
        diagnostics_log_directory: "/users/alice/private-state/diagnostics/logs",
      },
    } as DiagnosticsSnapshot;

    const result = await buildSupportBundle({
      snapshot: snapshotWithPrivateHostField,
      environment: {
        platform: "linux",
        architecture: "x64",
        vscode_version: "1.100.0",
        extension_version: "0.0.1-alpha.4",
      },
      diagnosticsLogDirectory: logDirectory,
      now: new Date("2026-07-18T10:00:00.000Z"),
    });
    const archiveText = Buffer.from(result.bytes).toString("utf8");

    expect(archiveText).not.toContain("/users/alice");
    expect(archiveText).not.toContain("alice-private-agent-name");
    expect(archiveText).toContain('"agent_id": "agent:1"');
  });

  it("creates a partial bundle when logs are missing or malformed", async () => {
    const logDirectory = await temporaryDirectory();
    await writeFile(path.join(logDirectory, "openaide-extension.jsonl"), [
      "not-json",
      JSON.stringify({
        timestamp_ms: 1e100,
        scope: "openaide-extension",
        level: "error",
        event: "invalid_timestamp",
        fields: {},
      }),
    ].join("\n"));

    const result = await buildSupportBundle({
      snapshot: snapshot(),
      environment: {
        platform: "linux",
        architecture: "x64",
        vscode_version: "1.100.0",
        extension_version: "0.0.1-alpha.4",
      },
      diagnosticsLogDirectory: logDirectory,
      now: new Date("2026-07-18T10:00:00.000Z"),
    });
    const entries = readStoredZip(result.bytes);

    expect(result.manifest.sources.extension_log).toEqual({
      status: "included",
      record_count: 0,
      discarded_record_count: 2,
    });
    expect(result.manifest.sources.app_server_log.status).toBe("unavailable");
    expect(entries.has("logs/openaide-app-server.jsonl")).toBe(false);
  });

  it("caps the exported bytes while retaining the newest complete records", async () => {
    const logDirectory = await temporaryDirectory();
    const records = Array.from({ length: 18_000 }, (_, index) => JSON.stringify({
      timestamp: "2026-07-18T09:50:00.000Z",
      scope: "openaide-extension",
      level: "warn",
      event: "app_server_spawn_failed",
      fields: { sequence_count: index },
    }));
    await writeFile(path.join(logDirectory, "openaide-extension.jsonl"), `${records.join("\n")}\n`);

    const result = await buildSupportBundle({
      snapshot: snapshot(),
      environment: {
        platform: "linux",
        architecture: "x64",
        vscode_version: "1.100.0",
        extension_version: "0.0.1-alpha.4",
      },
      diagnosticsLogDirectory: logDirectory,
      now: new Date("2026-07-18T10:00:00.000Z"),
    });
    const content = readStoredZip(result.bytes).get("logs/openaide-extension.jsonl")!;

    expect(Buffer.byteLength(content)).toBeLessThanOrEqual(2 * 1024 * 1024);
    expect(content).toContain('"sequence_count":17999');
  });
});

async function temporaryDirectory() {
  const directory = await mkdtemp(path.join(tmpdir(), "openaide-support-bundle-"));
  createdDirectories.push(directory);
  return directory;
}

function snapshot(): DiagnosticsSnapshot {
  return {
    created_at: "2026-07-18T10:00:00.000Z",
    runtime: {
      status: "ready",
      version: "0.1.0",
      method_count: 13,
      tasks: {
        visible_count: 1,
        total_count: 1,
        active_count: 0,
        active_tasks: [],
        revision: 4,
      },
      redaction: "prompt_text_file_contents_terminal_output_and_secrets_removed",
    },
    notices: [],
    process: {
      running: true,
      runtime_source_kind: "bundled",
      storage_root_kind: "extension-storage",
    },
  };
}

/** Reads the uncompressed ZIP entries produced at the Support Export boundary. */
function readStoredZip(bytes: Uint8Array) {
  const entries = new Map<string, string>();
  const buffer = Buffer.from(bytes);
  let offset = 0;
  while (buffer.readUInt32LE(offset) === 0x04034b50) {
    const compressedSize = buffer.readUInt32LE(offset + 18);
    const nameLength = buffer.readUInt16LE(offset + 26);
    const extraLength = buffer.readUInt16LE(offset + 28);
    const nameStart = offset + 30;
    const contentStart = nameStart + nameLength + extraLength;
    const name = buffer.subarray(nameStart, nameStart + nameLength).toString("utf8");
    entries.set(name, buffer.subarray(contentStart, contentStart + compressedSize).toString("utf8"));
    offset = contentStart + compressedSize;
  }
  return entries;
}
