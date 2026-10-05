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

async function killWindowsTree(child, spawnProcess, clock) {
  let killer;
  try {
    killer = spawnProcess("taskkill", ["/PID", String(child.pid), "/T", "/F"], {
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

/** Await observed closure; deadlines only bound a broken shutdown, never order it. */
export async function shutdownSmokeProcess(child, observed, {
  platform = process.platform,
  spawnProcess = spawn,
  clock = timers,
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
  if (platform === "win32") {
    // Keep stdin open until taskkill has addressed the live parent. Sending EOF
    // first can orphan descendants before taskkill discovers the owned tree.
    if (!observed.didClose && child.pid && child.exitCode === null && child.signalCode === null) {
      taskkill = await killWindowsTree(child, spawnProcess, clock);
      emit("tree_kill_finished", { taskkill });
      if (taskkill.closed === false) {
        emit("tree_kill_unclosed", { taskkill });
        throw new Error(`App Server cleanup helper did not close: ${JSON.stringify(taskkill)}; temporary smoke state was retained`);
      }
    }
    // A second wait without an intervening action adds no synchronization.
    // Preserve the previous total watchdog, but wait on one closure event.
    if (await waitForClose(observed, 15_000, clock)) {
      emit("closed", { taskkill });
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
  const diagnostics = { ...observed.snapshot(), ...(taskkill && { taskkill }) };
  emit("timed_out", { taskkill });
  throw new Error(`App Server shutdown timed out: ${JSON.stringify(diagnostics)}; temporary smoke state was retained`);
}
