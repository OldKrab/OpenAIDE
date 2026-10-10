import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { watch } from "node:fs";
import { mkdir, mkdtemp, readFile, readdir, rename, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { expiredSegment, parseProcessStat, primaryCheckoutRoot, safeRecord, selectWebShell, snapshot } from "./driver-profiler.mjs";

test("linked worktree sessions resolve the primary checkout's Driver capture", async () => {
  const fixture = await mkdtemp(path.join(tmpdir(), "openaide-profiler-worktree-test-"));
  const primary = path.join(fixture, "primary");
  const linked = path.join(fixture, "linked");
  try {
    execFileSync("git", ["init", "--initial-branch=shushakov/profiler-fixture", primary], { stdio: "ignore" });
    await writeFile(path.join(primary, "fixture.txt"), "fixture");
    execFileSync("git", ["-C", primary, "add", "fixture.txt"], { stdio: "ignore" });
    execFileSync("git", ["-C", primary, "-c", "user.name=Test Fixture", "-c", "user.email=test@example.invalid",
      "-c", "commit.gpgSign=false", "-c", "core.hooksPath=/dev/null", "commit", "-m", "fixture"], { stdio: "ignore" });
    execFileSync("git", ["-C", primary, "worktree", "add", "--detach", linked], { stdio: "ignore" });
    assert.equal(primaryCheckoutRoot(primary), primary);
    assert.equal(primaryCheckoutRoot(linked), primary);
    assert.equal(primaryCheckoutRoot(fixture), fixture);
  } finally { await rm(fixture, { recursive: true }); }
});

test("correlated timings survive while prompts, paths, arbitrary errors and URLs are omitted", () => {
  const record = safeRecord({ timestamp_ms: 2000, event: "rpc_request_completed", fields: {
    task_id: "task-1", request_id: "req:123", method: "task.send", duration_ms: 170,
    prompt: "private", path: "/private", error: "private failure", url: "https://private",
    error_kind: "https://private", output_bytes: 512,
  } }, "app_server", 1000);
  assert.deepEqual(record.fields, { task_id: "task-1", request_id: "req:123", method: "task.send", duration_ms: 170, output_bytes: 512 });
  const named = (method) => safeRecord({ timestamp_ms: 2000, event: "rpc_request_completed", fields: { method } }, "app_server", 1000).fields;
  assert.deepEqual(named("attachment/listDirectory"), { method: "attachment/listDirectory" });
  assert.deepEqual(named("/private/path"), {});
  assert.deepEqual(named("https://private/a"), {});
  assert.equal(safeRecord({ timestamp_ms: 999, event: "rpc_request_completed" }, "app_server", 1000), undefined);
});

test("retention preserves a boundary segment and never removes unrelated files", () => {
  const options = { windowMs: 600_000, segmentMs: 30_000 };
  assert.equal(expiredSegment("369999.jsonl", 1_000_000, options), true);
  assert.equal(expiredSegment("370001.cpuprofile", 1_000_000, options), false);
  assert.equal(expiredSegment("status.json", 1_000_000, options), false);
  assert.equal(expiredSegment("999.backup", 1_000_000, options), false);
});

test("process counters read the live proc boundary with a stable process identity", { skip: process.platform !== "linux" }, async () => {
  const info = parseProcessStat(await readFile(`/proc/${process.pid}/stat`, "utf8"));
  assert.ok(info.cpu_ticks >= 0);
  assert.ok(info.rss_pages > 0);
  assert.match(info.start_ticks, /^\d+$/);
  assert.match(info.state, /^[A-Z]$/);
});

test("snapshot freezes the last ten minutes and excludes old records and incomplete writes", async () => {
  const fixture = await mkdtemp(path.join(tmpdir(), "openaide-profiler-test-"));
  const root = path.join(fixture, "capture");
  await mkdir(root);
  try {
    const now = Date.now();
    const file = `${now - 20_000}.jsonl`;
    await writeFile(path.join(root, file), [JSON.stringify({ timestamp_ms: now - 700_000 }), JSON.stringify({ timestamp_ms: now }), '{"timestamp_ms":'].join("\n"));
    await writeFile(path.join(root, `${now - 700_000}.jsonl`), "{}\n");
    await writeFile(path.join(root, "status.json"), JSON.stringify({ web_cpu_profiles: true }));
    const saved = await snapshot({ root, windowMs: 600_000, segmentMs: 30_000 });
    const lines = (await readFile(path.join(saved, file), "utf8")).trim().split("\n").map(JSON.parse);
    assert.deepEqual(lines, [{ timestamp_ms: now }]);
    assert.deepEqual((await readdir(saved)).sort(), [file, "manifest.json"].sort());
    assert.ok(JSON.parse(await readFile(path.join(saved, "manifest.json"), "utf8")).web_cpu_profiles);
    await rm(saved, { recursive: true });
  } finally { await rm(fixture, { recursive: true }); }
});

test("snapshot flush request waits for collector acknowledgment even during startup", async () => {
  const fixture = await mkdtemp(path.join(tmpdir(), "openaide-profiler-flush-test-"));
  const root = path.join(fixture, "capture");
  await mkdir(root);
  await writeFile(path.join(root, "status.json"), "{}");
  let watcher;
  try {
    const requested = new Promise(resolve => {
      watcher = watch(root, (_event, name) => { if (name === "flush-request.json") resolve(); });
    });
    const saving = snapshot({ root, flush: true, windowMs: 600_000, segmentMs: 30_000 });
    await requested;
    const timestamp = Number(await readFile(path.join(root, "flush-request.json"), "utf8"));
    const profile = `${timestamp}.cpuprofile`;
    await writeFile(path.join(root, profile), JSON.stringify({ captureStartedAt: timestamp, nodes: [] }));
    await writeFile(path.join(root, "status.tmp"), JSON.stringify({ last_cpu_flush_ms: timestamp, web_cpu_profiles: true }));
    await rename(path.join(root, "status.tmp"), path.join(root, "status.json"));
    const saved = await saving;
    assert.equal(JSON.parse(await readFile(path.join(saved, "manifest.json"), "utf8")).cpu_flush_outcome, "success");
    assert.ok((await readdir(saved)).includes(profile));
  } finally { watcher?.close(); await rm(fixture, { recursive: true }); }
});

test("a transient process with Web Shell argv never displaces the attached Web Shell", () => {
  // The attached inspector target stays selected while it lives, whatever pid order the cgroup lists.
  assert.equal(selectWebShell([{ pid: 416812, ppid: 1 }, { pid: 936412, ppid: 1 }], 416812), 416812);
  assert.equal(selectWebShell([{ pid: 936412, ppid: 1 }, { pid: 416812, ppid: 1 }], 416812), 416812);
});

test("without an attachment, the Web Shell is the root of its own process family", () => {
  assert.equal(selectWebShell([{ pid: 936412, ppid: 416812 }, { pid: 416812, ppid: 1 }]), 416812);
  assert.equal(selectWebShell([{ pid: 936412, ppid: 1 }, { pid: 416812, ppid: 1 }]), 416812);
  assert.equal(selectWebShell([]), undefined);
});

test("process stat exposes the parent pid", () => {
  assert.equal(parseProcessStat("12 (node) S 7 12 12 0 -1 0 0 0 0 0 5 6 0 0 20 0 1 0 99 0 321").ppid, 7);
});
