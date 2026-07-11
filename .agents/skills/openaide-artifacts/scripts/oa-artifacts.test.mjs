import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const scriptPath = path.join(path.dirname(fileURLToPath(import.meta.url)), "oa-artifacts.mjs");

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

  assert.equal(parsed.roots.length, 1);
  assert.equal(parsed.roots[0].runtimeRoot, state);
  assert.equal(parsed.roots[0].tasksRoot, path.join(state, "tasks"));
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
