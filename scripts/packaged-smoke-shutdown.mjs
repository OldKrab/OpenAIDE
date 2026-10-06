import { spawn } from "node:child_process";

/** Observe at spawn time: exit is not closure when a descendant still owns stdio. */
export function observeSmokeProcess(child) {
  let didClose = false;
  let spawnError;
  const closed = new Promise((resolve) => {
    child.once("close", () => { didClose = true; resolve(); });
  });
  child.once("error", (error) => { spawnError = error.code ?? "unknown"; });
  return {
    closed,
    get didClose() { return didClose; },
    snapshot: () => ({
      exit: child.exitCode, signal: child.signalCode, closed: didClose,
      spawn_error: spawnError ?? null,
      stdout_open: Boolean(child.stdout && !child.stdout.destroyed && !child.stdout.readableEnded),
      stderr_open: Boolean(child.stderr && !child.stderr.destroyed && !child.stderr.readableEnded),
    }),
  };
}

const timers = { set: setTimeout, clear: clearTimeout };

async function waitForClose(observed, timeoutMs, clock) {
  if (observed.didClose) return true;
  let timer;
  try {
    return await Promise.race([
      observed.closed.then(() => true),
      new Promise((resolve) => { timer = clock.set(() => resolve(false), timeoutMs); }),
    ]);
  } finally {
    clock.clear(timer);
  }
}

async function killWindowsTree(pid, spawnProcess, clock) {
  let killer;
  try {
    killer = spawnProcess("taskkill", ["/PID", String(pid), "/T", "/F"], {
      stdio: ["ignore", "pipe", "pipe"], windowsHide: true,
    });
  } catch (error) {
    return { outcome: "spawn_failed", error_code: error.code ?? "unknown" };
  }
  const observed = observeSmokeProcess(killer);
  let stdoutBytes = 0;
  let stderrBytes = 0;
  // Drain both pipes but retain only metadata: localized taskkill output is not
  // an API and may contain machine-specific information.
  killer.stdout?.on("data", (chunk) => { stdoutBytes += Buffer.byteLength(chunk); });
  killer.stderr?.on("data", (chunk) => { stderrBytes += Buffer.byteLength(chunk); });
  const timedOut = !await waitForClose(observed, 5_000, clock);
  if (timedOut) {
    killer.kill("SIGKILL");
    // The helper is owned too. Sending a kill is not an acknowledgement that
    // it exited; bound and join its closure before declaring cleanup complete.
    await waitForClose(observed, 5_000, clock);
  }
  const result = observed.snapshot();
  return {
    outcome: timedOut ? "timed_out" : result.spawn_error ? "spawn_failed" : "closed",
    closed: result.closed,
    exit: result.exit, signal: result.signal, error_code: result.spawn_error,
    stdout_bytes: stdoutBytes, stderr_bytes: stderrBytes,
  };
}

const LIST_PROCESSES = "Get-CimInstance Win32_Process | ForEach-Object { '{0} {1} {2} {3}' -f "
  + "$_.ProcessId, $_.ParentProcessId, $(if ($_.CreationDate) { $_.CreationDate.ToFileTimeUtc() } else { 0 }), $_.Name }";

/** Lists every process as `{ pid, ppid, created, name }`, or null when the listing is unavailable. */
async function listWindowsProcesses(spawnProcess, clock) {
  let lister;
  try {
    lister = spawnProcess("powershell", ["-NoProfile", "-NonInteractive", "-Command", LIST_PROCESSES], {
      stdio: ["ignore", "pipe", "pipe"], windowsHide: true,
    });
  } catch {
    return null;
  }
  const observed = observeSmokeProcess(lister);
  let stdout = "";
  lister.stdout?.on("data", (chunk) => { stdout += chunk; });
  lister.stderr?.resume();
  if (!await waitForClose(observed, 30_000, clock)) {
    lister.kill("SIGKILL");
    await waitForClose(observed, 5_000, clock);
    return null;
  }
  if (observed.snapshot().spawn_error || lister.exitCode !== 0) return null;
  return stdout.split(/\r?\n/).flatMap((line) => {
    const match = /^(\d+) (\d+) (\d+) (.+)$/.exec(line.trim());
    return match
      ? [{ pid: Number(match[1]), ppid: Number(match[2]), created: BigInt(match[3]), name: match[4] }]
      : [];
  });
}

const SWEEP_PASSES = 5;

/**
 * Ends descendants that left the tree `taskkill /T` walked. That walk follows
 * live parent links in one snapshot, so it misses a process spawned while it
 * runs and one whose parent had already exited. Windows keeps a dead parent's
 * pid on its children, so both remain attributable: a process belongs to the
 * tree when its parent does and it was created no earlier than that parent,
 * which also excludes a reused pid. A descendant whose parent exited before
 * `before` was taken is outside this attribution; `orphans_since_start`
 * names the candidates when closure still does not arrive.
 */
async function sweepWindowsDescendants(rootPid, before, listProcesses, spawnProcess, clock) {
  const root = before?.find((entry) => entry.pid === rootPid);
  if (!root) return { outcome: "unlisted" };
  const owned = new Map([[root.pid, root.created]]);
  const adopt = (processes) => {
    for (let grew = true; grew;) {
      grew = false;
      for (const entry of processes) {
        const parentCreated = owned.get(entry.ppid);
        if (owned.has(entry.pid) || parentCreated === undefined || entry.created < parentCreated) continue;
        owned.set(entry.pid, entry.created);
        grew = true;
      }
    }
  };
  adopt(before);
  let killed = 0;
  let alive = [];
  for (let pass = 1; pass <= SWEEP_PASSES; pass += 1) {
    const now = await listProcesses();
    if (!now) return { outcome: "list_failed", killed };
    adopt(now);
    alive = now.filter((entry) => owned.get(entry.pid) === entry.created);
    if (alive.length === 0) {
      const living = new Set(now.map((entry) => entry.pid));
      // Names only: these are the processes that could still hold the pipes.
      const orphans = now
        .filter((entry) => entry.created >= root.created && !living.has(entry.ppid))
        .map((entry) => entry.name);
      return { outcome: "clear", passes: pass, killed, orphans_since_start: orphans.slice(0, 20) };
    }
    for (const entry of alive) {
      await killWindowsTree(entry.pid, spawnProcess, clock);
      killed += 1;
    }
  }
  return { outcome: "survivors", killed, survivors: alive.map((entry) => entry.name).slice(0, 20) };
}

/** Await observed closure; deadlines only bound a broken shutdown, never order it. */
export async function shutdownSmokeProcess(child, observed, {
  platform = process.platform,
  spawnProcess = spawn,
  clock = timers,
  listProcesses = () => listWindowsProcesses(spawnProcess, clock),
  onEvent = () => {},
} = {}) {
  const started = performance.now();
  const emit = (outcome, extra = {}) => onEvent({
    operation: "packaged_smoke_shutdown", outcome,
    duration_ms: Math.round(performance.now() - started),
    ...observed.snapshot(), ...extra,
  });
  emit("started");
  let taskkill;
  let sweep;
  if (platform === "win32") {
    // Keep stdin open until taskkill has addressed the live parent. Sending EOF
    // first can orphan descendants before taskkill discovers the owned tree.
    if (!observed.didClose && child.pid && child.exitCode === null && child.signalCode === null) {
      const before = await listProcesses();
      taskkill = await killWindowsTree(child.pid, spawnProcess, clock);
      emit("tree_kill_finished", { taskkill });
      if (taskkill.closed === false) {
        emit("tree_kill_unclosed", { taskkill });
        throw new Error(`App Server cleanup helper did not close: ${JSON.stringify(taskkill)}; temporary smoke state was retained`);
      }
      sweep = await sweepWindowsDescendants(child.pid, before, listProcesses, spawnProcess, clock);
      emit("tree_sweep_finished", { sweep });
    }
    // A second wait without an intervening action adds no synchronization.
    // Preserve the previous total watchdog, but wait on one closure event.
    if (await waitForClose(observed, 15_000, clock)) {
      emit("closed", { taskkill, sweep });
      return;
    }
  } else {
    if (!child.stdin.destroyed) child.stdin.end();
    if (await waitForClose(observed, 10_000, clock)) {
      emit("closed");
      return;
    }
    if (child.exitCode === null && child.signalCode === null) child.kill("SIGKILL");
    emit("force_kill_requested");
    if (await waitForClose(observed, 5_000, clock)) {
      emit("closed");
      return;
    }
  }
  const diagnostics = { ...observed.snapshot(), ...(taskkill && { taskkill }), ...(sweep && { sweep }) };
  emit("timed_out", { taskkill, sweep });
  throw new Error(`App Server shutdown timed out: ${JSON.stringify(diagnostics)}; temporary smoke state was retained`);
}
