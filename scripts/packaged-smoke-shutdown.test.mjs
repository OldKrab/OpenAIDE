import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { EventEmitter, once } from "node:events";
import { PassThrough } from "node:stream";
import test from "node:test";
import { observeSmokeProcess, shutdownSmokeProcess } from "./packaged-smoke-shutdown.mjs";

// Deadline expiry is an explicit test input, not a sleep or a scheduling guess.
function controlledClock() {
  const pending = new Set();
  const created = [];
  let next;
  return {
    pending,
    set(callback, ms) {
      const timer = { ms, expire: callback };
      pending.add(timer);
      if (next) { next.resolve(timer); next = undefined; }
      else created.push(timer);
      return timer;
    },
    clear(timer) { pending.delete(timer); },
    next() {
      if (created.length) return Promise.resolve(created.shift());
      assert.equal(next, undefined, "one deadline observer at a time");
      next = Promise.withResolvers();
      return next.promise;
    },
  };
}

function processFixture() {
  const child = new EventEmitter();
  Object.assign(child, {
    pid: 42, exitCode: null, signalCode: null,
    stdin: new PassThrough(), stdout: new PassThrough(), stderr: new PassThrough(),
    killed: [],
    kill(signal) { this.killed.push(signal); return true; },
    exit(code = 0, signal = null) {
      this.exitCode = code; this.signalCode = signal; this.emit("exit", code, signal);
    },
    close(code = 0, signal = null) {
      this.exit(code, signal);
      this.stdout.destroy(); this.stderr.destroy();
      this.emit("close", code, signal);
    },
  });
  return child;
}

// The process listing is unavailable, so only the tree kill addresses the tree.
const noListing = async () => null;

/** A taskkill fixture that closes as soon as it is spawned and records its target. */
function closingKillers(targets) {
  return (command, args) => {
    assert.equal(command, "taskkill");
    targets.push(Number(args[1]));
    const killer = processFixture();
    queueMicrotask(() => killer.close());
    return killer;
  };
}

test("Windows ends descendants that left the tree taskkill walked", async () => {
  const child = processFixture();
  const observed = observeSmokeProcess(child);
  const clock = controlledClock();
  const events = [];
  const root = { pid: 42, ppid: 1, created: 100n, name: "app-server.exe" };
  const adapter = { pid: 50, ppid: 42, created: 110n, name: "node.exe" };
  // Spawned by the adapter while the tree kill ran, and outlives its parent.
  const escaped = { pid: 60, ppid: 50, created: 120n, name: "codex.exe" };
  // A reused parent pid: created before the owned process that now has it.
  const unrelated = { pid: 70, ppid: 50, created: 105n, name: "unrelated.exe" };
  const listings = [[root, adapter, unrelated], [escaped, unrelated], [unrelated]];
  const targets = [];
  const stopping = shutdownSmokeProcess(child, observed, {
    platform: "win32", clock, onEvent: (event) => events.push(event),
    listProcesses: async () => listings.shift(),
    spawnProcess: closingKillers(targets),
  });
  assert.equal((await clock.next()).ms, 5_000);
  assert.equal((await clock.next()).ms, 5_000);
  assert.equal((await clock.next()).ms, 15_000);
  child.close(1);
  await stopping;
  assert.deepEqual(targets, [42, 60]);
  const sweep = events.find((event) => event.outcome === "tree_sweep_finished").sweep;
  assert.deepEqual(sweep, { outcome: "clear", passes: 2, killed: 1, orphans_since_start: ["unrelated.exe"] });
  assert.equal(clock.pending.size, 0);
});

test("Windows names descendants it could not end", async () => {
  const child = processFixture();
  const observed = observeSmokeProcess(child);
  const clock = controlledClock();
  const root = { pid: 42, ppid: 1, created: 100n, name: "app-server.exe" };
  const stuck = { pid: 50, ppid: 42, created: 110n, name: "node.exe" };
  const targets = [];
  const stopping = shutdownSmokeProcess(child, observed, {
    platform: "win32", clock,
    listProcesses: async () => [root, stuck],
    spawnProcess: closingKillers(targets),
  });
  const rejected = assert.rejects(stopping, /"sweep":\{"outcome":"survivors","killed":10,"survivors":\["app-server.exe","node.exe"\]\}/);
  for (let helper = 0; helper < 11; helper += 1) assert.equal((await clock.next()).ms, 5_000);
  const closureDeadline = await clock.next();
  child.exit(1);
  closureDeadline.expire();
  await rejected;
  assert.equal(clock.pending.size, 0);
});

test("Windows waits for App Server close after taskkill closes, without sending EOF", async () => {
  const child = processFixture();
  const observed = observeSmokeProcess(child);
  const killer = processFixture();
  const clock = controlledClock();
  const events = [];
  let completed = false;
  const stopping = shutdownSmokeProcess(child, observed, {
    platform: "win32", clock, listProcesses: noListing, onEvent: (event) => events.push(event),
    spawnProcess(command, args, options) {
      assert.equal(command, "taskkill");
      assert.deepEqual(args, ["/PID", "42", "/T", "/F"]);
      assert.deepEqual(options.stdio, ["ignore", "pipe", "pipe"]);
      return killer;
    },
  }).then(() => { completed = true; });
  assert.equal((await clock.next()).ms, 5_000);
  killer.close();
  assert.equal((await clock.next()).ms, 15_000);
  assert.equal(completed, false, "taskkill completion does not acknowledge pipe closure");
  assert.equal(child.stdin.writableEnded, false, "keep the live parent addressable");
  child.exit(1);
  assert.equal(observed.didClose, false, "root exit is not tree closure");
  child.close(1);
  await stopping;
  assert.equal(clock.pending.size, 0);
  assert.deepEqual(events.map((event) => event.outcome), ["started", "tree_kill_finished", "tree_sweep_finished", "closed"]);
});

test("Windows accepts a concurrent close even when taskkill reports a missing process", async () => {
  const child = processFixture();
  const observed = observeSmokeProcess(child);
  const killer = processFixture();
  const clock = controlledClock();
  const stopping = shutdownSmokeProcess(child, observed, {
    platform: "win32", clock, listProcesses: noListing, spawnProcess: () => killer,
  });
  await clock.next();
  child.close();
  killer.close(128);
  await stopping;
  assert.equal(clock.pending.size, 0);
});

for (const scenario of ["nonzero", "spawn_error", "timeout"]) {
  test(`Windows preserves taskkill ${scenario} diagnostics when stdio never closes`, async () => {
    const child = processFixture();
    const observed = observeSmokeProcess(child);
    const killer = processFixture();
    const clock = controlledClock();
    const stopping = shutdownSmokeProcess(child, observed, {
      platform: "win32", clock, listProcesses: noListing, spawnProcess: () => killer,
    });
    const rejected = assert.rejects(stopping, (error) => {
      assert.match(error.message, /temporary smoke state was retained/);
      assert.match(error.message, /"closed":false/);
      assert.match(error.message, /"stdout_open":true/);
      if (scenario === "nonzero") {
        assert.match(error.message, /"exit":128/);
        assert.match(error.message, /"stderr_bytes":7/);
        assert.doesNotMatch(error.message, /private/);
      } else if (scenario === "spawn_error") assert.match(error.message, /"error_code":"ENOENT"/);
      else assert.match(error.message, /"outcome":"timed_out"/);
      return true;
    });
    const killerDeadline = await clock.next();
    if (scenario === "timeout") killerDeadline.expire();
    else {
      if (scenario === "spawn_error") killer.emit("error", Object.assign(new Error("private"), { code: "ENOENT" }));
      killer.stderr.write("private");
      killer.close(128);
    }
    if (scenario === "timeout") {
      await clock.next();
      assert.deepEqual(killer.killed, ["SIGKILL"]);
      killer.close(null, "SIGKILL");
    }
    const closureDeadline = await clock.next();
    child.exit(1);
    closureDeadline.expire();
    await rejected;
    assert.equal(clock.pending.size, 0);
  });
}

test("already closed Windows process needs neither taskkill nor a watchdog", async () => {
  const child = processFixture();
  const observed = observeSmokeProcess(child);
  child.close();
  await shutdownSmokeProcess(child, observed, {
    platform: "win32",
    spawnProcess() { assert.fail("must not target a stale PID"); },
    clock: { set() { assert.fail("must not wait after closure"); } },
  });
});

test("Windows reports open stdio after root exit without targeting a stale PID", async () => {
  const child = processFixture();
  const observed = observeSmokeProcess(child);
  child.exit(1);
  const clock = controlledClock();
  let killCalls = 0;
  const stopping = shutdownSmokeProcess(child, observed, {
    platform: "win32", clock,
    spawnProcess() { killCalls += 1; throw new Error("unexpected taskkill"); },
  });
  const rejected = assert.rejects(stopping, /"exit":1.*"closed":false.*"stdout_open":true/);
  (await clock.next()).expire();
  await rejected;
  assert.equal(killCalls, 0, "a dead root cannot identify its former tree");
  assert.equal(clock.pending.size, 0);
});

test("cannot report successful cleanup while a killed taskkill helper remains unclosed", async () => {
  const child = processFixture();
  const observed = observeSmokeProcess(child);
  const killer = processFixture();
  const clock = controlledClock();
  const stopping = shutdownSmokeProcess(child, observed, {
    platform: "win32", clock, listProcesses: noListing, spawnProcess: () => killer,
  });
  const rejected = assert.rejects(stopping, /cleanup helper did not close.*"closed":false/);
  child.close();
  (await clock.next()).expire();
  (await clock.next()).expire();
  await rejected;
  assert.equal(clock.pending.size, 0);
});

test("POSIX stalled shutdown escalates on its watchdog and still requires close", async () => {
  const child = processFixture();
  const observed = observeSmokeProcess(child);
  const clock = controlledClock();
  const stopping = shutdownSmokeProcess(child, observed, { platform: "linux", clock });
  assert.equal(child.stdin.writableEnded, true);
  const graceful = await clock.next();
  assert.equal(graceful.ms, 10_000);
  assert.deepEqual(child.killed, []);
  graceful.expire();
  assert.equal((await clock.next()).ms, 5_000);
  assert.deepEqual(child.killed, ["SIGKILL"]);
  child.close(null, "SIGKILL");
  await stopping;
  assert.equal(clock.pending.size, 0);
});

async function startPipeTree(t) {
  // No sleeps: the descendant acknowledges that it owns the inherited handles
  // before the parent is told to exit. Its open server keeps it alive after IPC
  // disconnect, reproducing the Windows failure's exit-without-close boundary.
  const descendantSource = `
    const server = require("node:net").createServer();
    server.listen(0, "127.0.0.1", () => process.send({ ready: true, pid: process.pid }));
  `;
  const parentSource = `
    const { spawn } = require("node:child_process");
    const child = spawn(process.execPath, ["-e", ${JSON.stringify(descendantSource)}], {
      stdio: ["ignore", "inherit", "inherit", "ipc"],
    });
    child.on("message", (message) => process.send(message));
    process.on("message", () => process.exit(1));
  `;
  const child = spawn(process.execPath, ["-e", parentSource], {
    stdio: ["pipe", "pipe", "pipe", "ipc"], windowsHide: true,
    detached: process.platform !== "win32",
  });
  const observed = observeSmokeProcess(child);
  child.stdout.resume(); child.stderr.resume();
  let descendantPid;
  t.after(async () => {
    // Own cleanup even if cancellation precedes the descendant's ready message.
    if (child.pid) {
      if (process.platform === "win32") {
        if (child.exitCode === null && child.signalCode === null) {
          spawnSync("taskkill", ["/PID", String(child.pid), "/T", "/F"], {
            stdio: "ignore", windowsHide: true, timeout: 30_000,
          });
        }
      } else {
        try { process.kill(-child.pid, "SIGKILL"); }
        catch (error) { if (error.code !== "ESRCH") throw error; }
      }
    }
    for (const pid of [descendantPid, child.exitCode === null ? child.pid : undefined]) {
      if (!pid) continue;
      try { process.kill(pid, "SIGKILL"); } catch (error) { if (error.code !== "ESRCH") throw error; }
    }
    // Test failure cleanup must not itself hang if fixture setup was interrupted.
    // Production shutdown still requires genuine closure without destroying pipes.
    child.stdin.destroy(); child.stdout.destroy(); child.stderr.destroy();
    if (child.connected) child.disconnect();
    await Promise.race([
      observed.closed,
      new Promise((_, reject) => {
        const timer = setTimeout(() => reject(new Error("fixture cleanup did not close")), 30_000);
        observed.closed.then(() => clearTimeout(timer));
      }),
    ]);
  });
  const [ready] = await once(child, "message", { signal: t.signal });
  assert.equal(ready.ready, true);
  descendantPid = ready.pid;
  return { child, observed, descendantPid };
}

// Node's Windows stdio transport does not retain this pipe on root exit. Keep
// the actual inherited-POSIX-pipe reproduction here; the controlled-event test
// above covers the dead-root/open-stdio invariant on every platform.
test("POSIX inherited pipe distinguishes root exit from closure using IPC barriers", {
  skip: process.platform === "win32", timeout: 30_000,
}, async (t) => {
  const { child, observed } = await startPipeTree(t);
  const exited = once(child, "exit", { signal: t.signal });
  child.send("exit");
  await exited;
  assert.equal(observed.didClose, false);
  const clock = controlledClock();
  let killCalls = 0;
  const stopping = shutdownSmokeProcess(child, observed, {
    platform: "win32", clock,
    spawnProcess() { killCalls += 1; throw new Error("unexpected taskkill"); },
  });
  const rejected = assert.rejects(stopping, /"exit":1.*"closed":false.*"stdout_open":true/);
  (await clock.next()).expire();
  await rejected;
  assert.equal(killCalls, 0, "a dead root cannot identify its former tree");
});

test("native Windows taskkill closes an IPC-ready App Server tree", {
  skip: process.platform !== "win32", timeout: 30_000,
}, async (t) => {
  const { child, observed, descendantPid } = await startPipeTree(t);
  await shutdownSmokeProcess(child, observed);
  assert.equal(observed.didClose, true);
  assert.throws(() => process.kill(descendantPid, 0), { code: "ESRCH" },
    "the inherited-pipe owner must be dead, not merely disconnected");
});
