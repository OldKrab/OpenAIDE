import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const scriptPath = path.join(path.dirname(fileURLToPath(import.meta.url)), "oa-artifacts.mjs");

test("split store discovers target state and replays committed chat deltas", () => {
  const repo = fs.mkdtempSync(path.join(os.tmpdir(), "oa-artifacts-repo-"));
  const state = path.join(repo, ".openaide-web-target", "state");
  const taskDir = path.join(state, "task-store-v1", "tasks", "task_split");
  fs.mkdirSync(taskDir, { recursive: true });
  fs.writeFileSync(
    path.join(taskDir, "task.json"),
    JSON.stringify({
      schemaVersion: 1,
      storageSequence: 2,
      chatSequence: 1,
      chatSnapshot: "chat.snapshot.generation",
      chatJournal: "chat.journal.generation",
      task: {
        task_id: "task_split",
        status: "inactive",
        title: { value: "Split task", source: "agent" },
        message_history_version: 2,
        last_activity: "2026-07-23T00:00:00.000Z",
      },
    }),
  );
  fs.writeFileSync(
    path.join(taskDir, "chat.snapshot.generation"),
    JSON.stringify({
      schemaVersion: 1,
      messages: [storedAgentMessage(1, "agent:one", "before")],
      messageMeta: {
        task_id: "task_split",
        version: 1,
        message_count: 1,
        first_cursor: "m:1",
        last_cursor: "m:1",
      },
      artifactHeads: {},
    }),
  );
  writeJournal(path.join(taskDir, "chat.journal.generation"), [
    {
      format_version: 1,
      sequence: 1,
      operations: [
        {
          operation: "append_message",
          message: storedAgentMessage(2, "agent:two", "after"),
        },
        {
          operation: "replace_message_meta",
          message_meta: {
            task_id: "task_split",
            version: 2,
            message_count: 2,
            first_cursor: "m:1",
            last_cursor: "m:2",
          },
        },
      ],
    },
  ]);

  const roots = JSON.parse(
    execFileSync(process.execPath, [scriptPath, "roots", "--json"], {
      cwd: repo,
      encoding: "utf8",
    }),
  );
  const tasks = JSON.parse(
    execFileSync(process.execPath, [scriptPath, "tasks", "--root", state, "--json"], {
      cwd: repo,
      encoding: "utf8",
    }),
  );
  const messages = JSON.parse(
    execFileSync(process.execPath, [scriptPath, "messages", "task_split", "--root", state, "--json"], {
      cwd: repo,
      encoding: "utf8",
    }),
  );
  const doctor = JSON.parse(
    execFileSync(process.execPath, [scriptPath, "doctor", "task_split", "--root", state, "--json"], {
      cwd: repo,
      encoding: "utf8",
    }),
  );
  const taskList = execFileSync(process.execPath, [scriptPath, "tasks", "--root", state], {
    cwd: repo,
    encoding: "utf8",
  });
  const search = JSON.parse(
    execFileSync(process.execPath, [scriptPath, "search", "after", "--root", state, "--json"], {
      cwd: repo,
      encoding: "utf8",
    }),
  );

  const discovered = roots.roots.find((root) => root.runtimeRoot === state);
  assert.ok(discovered);
  assert.equal(discovered.tasksRoot, path.join(state, "task-store-v1", "tasks"));
  assert.equal(tasks.tasks[0].task.task_id, "task_split");
  assert.equal(tasks.tasks[0].storage.layout, "split-v1");
  assert.match(taskList, /Split task/);
  assert.match(taskList, /messages=lazy/);
  assert.doesNotMatch(taskList, /\[object Object\]/);
  assert.deepEqual(
    messages.messages.map((stored) => stored.chat.message.parts[0].text),
    ["before", "after"],
  );
  assert.equal(search.hits.length, 1);
  assert.equal(search.hits[0].task_id, "task_split");
  assert.equal(doctor.finding_count, 0);

  const taskFile = path.join(taskDir, "task.json");
  const damagedMetadata = JSON.parse(fs.readFileSync(taskFile, "utf8"));
  damagedMetadata.chatSequence = 2;
  fs.writeFileSync(taskFile, JSON.stringify(damagedMetadata));
  const damagedDoctor = JSON.parse(
    execFileSync(process.execPath, [scriptPath, "doctor", "task_split", "--root", state, "--json"], {
      cwd: repo,
      encoding: "utf8",
    }),
  );
  assert.ok(damagedDoctor.findings.some((finding) => finding.code === "chat-storage-invalid"));
});

test("roots discovers repo-local web app state", () => {
  const repo = fs.mkdtempSync(path.join(os.tmpdir(), "oa-artifacts-repo-"));
  const state = path.join(repo, ".openaide-web-dev", "state");
  const taskDir = path.join(state, "tasks", "task_example");
  fs.mkdirSync(taskDir, { recursive: true });
  fs.writeFileSync(
    path.join(taskDir, "task.json"),
    JSON.stringify({
      task_id: "task_example",
      status: "inactive",
      title: "Example",
      last_activity: "2026-07-03T00:00:00.000Z",
    }),
  );

  const output = execFileSync(process.execPath, [scriptPath, "roots", "--json"], {
    cwd: repo,
    encoding: "utf8",
  });
  const parsed = JSON.parse(output);

  const discovered = parsed.roots.find((root) => root.runtimeRoot === state);
  assert.ok(discovered);
  assert.equal(discovered.tasksRoot, path.join(state, "tasks"));
});

test("runtime logs reads current and legacy runtime log names", () => {
  const repo = fs.mkdtempSync(path.join(os.tmpdir(), "oa-artifacts-repo-"));
  const state = path.join(repo, ".openaide-web-dev", "state");
  const taskDir = path.join(state, "tasks", "task_example");
  const logDir = path.join(state, "diagnostics", "logs");
  fs.mkdirSync(taskDir, { recursive: true });
  fs.mkdirSync(logDir, { recursive: true });
  fs.writeFileSync(path.join(taskDir, "task.json"), JSON.stringify({ task_id: "task_example", status: "inactive" }));
  fs.writeFileSync(
    path.join(logDir, "openaide-runtime.jsonl"),
    `${JSON.stringify({ timestamp: "2026-07-03T00:00:00.000Z", level: "error", event: "current_log_name_failed" })}\n`,
  );

  const output = execFileSync(process.execPath, [scriptPath, "failures", "runtime"], {
    cwd: repo,
    encoding: "utf8",
  });

  assert.match(output, /current_log_name_failed/);
});

test("--root limits discovery to the requested root", () => {
  const repo = fs.mkdtempSync(path.join(os.tmpdir(), "oa-artifacts-repo-"));
  const selected = path.join(repo, ".openaide-web-dev", "state");
  const other = path.join(repo, ".openaide-web-dev-other", "state");
  for (const [root, taskId] of [
    [selected, "task_selected"],
    [other, "task_other"],
  ]) {
    const taskDir = path.join(root, "tasks", taskId);
    fs.mkdirSync(taskDir, { recursive: true });
    fs.writeFileSync(path.join(taskDir, "task.json"), JSON.stringify({ task_id: taskId, status: "inactive" }));
  }

  const output = execFileSync(process.execPath, [scriptPath, "roots", "--root", selected, "--json"], {
    cwd: repo,
    encoding: "utf8",
  });
  const parsed = JSON.parse(output);

  assert.equal(parsed.roots.length, 1);
  assert.equal(parsed.roots[0].runtimeRoot, selected);
});

test("doctor reports stale active turns", () => {
  const repo = fs.mkdtempSync(path.join(os.tmpdir(), "oa-artifacts-repo-"));
  const state = path.join(repo, ".openaide-web-dev", "state");
  const taskDir = path.join(state, "tasks", "task_stuck");
  fs.mkdirSync(taskDir, { recursive: true });
  fs.writeFileSync(
    path.join(taskDir, "task.json"),
    JSON.stringify({
      task_id: "task_stuck",
      status: "active",
      active_turn_id: "turn_stuck",
      last_activity: String(Date.now() - 60 * 60 * 1000),
      message_history_version: 1,
    }),
  );
  fs.writeFileSync(path.join(taskDir, "message_meta.json"), JSON.stringify({ version: 1, message_count: 0 }));
  fs.writeFileSync(path.join(taskDir, "messages.jsonl"), "");

  const output = execFileSync(process.execPath, [scriptPath, "doctor"], {
    cwd: repo,
    encoding: "utf8",
  });

  assert.match(output, /stale-active-turn/);
  assert.match(output, /turn_stuck/);
});

function storedAgentMessage(sequence, identity, text) {
  return {
    sequence,
    chat: {
      cursor: `m:${sequence}`,
      identity,
      message_type: "agent_message",
      message_id: identity,
      message: {
        kind: "agent_message",
        id: identity,
        role: "agent",
        parts: [{ kind: "text", text }],
        created_at: "2026-07-23T00:00:00.000Z",
      },
    },
  };
}

function writeJournal(file, frames) {
  const chunks = [Buffer.from("OAIDETJ\0"), Buffer.from([1, 0])];
  for (const frame of frames) {
    const payload = Buffer.from(JSON.stringify(frame));
    const length = Buffer.alloc(8);
    length.writeBigUInt64LE(BigInt(payload.length));
    const checksum = Buffer.alloc(4);
    checksum.writeUInt32LE(crc32(payload));
    chunks.push(length, payload, checksum);
  }
  fs.writeFileSync(file, Buffer.concat(chunks));
}

function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1) {
      crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1));
    }
  }
  return (~crc) >>> 0;
}
