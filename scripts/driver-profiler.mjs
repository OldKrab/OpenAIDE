import { appendFile, chmod, copyFile, mkdir, open, readFile, readdir, rename, rm, writeFile } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { setTimeout as delay } from "node:timers/promises";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const defaults = {
  unit: "openaide-web-driver-5474.service",
  flush: true,
  root: path.join(repo, ".openaide-web-dev/state/diagnostics/driver-profiler"),
  logs: [path.join(repo, ".openaide-web-dev/state/diagnostics/logs/openaide-app-server.jsonl"), "/tmp/openaide-web-driver-5474.log"],
  windowMs: 600_000,
  segmentMs: 30_000,
};
const maxSegmentBytes = 8 * 1024 * 1024;
const maxReadBytes = 1024 * 1024;
const fields = new Set(("task_id turn_id session_id native_session_id request_id client_request_id connection_id client_id operation_id agent_id method operation outcome outcome_kind response_kind reason reason_code stage status previous_status error_kind error_code error_recoverable attempt retry_count duration_ms elapsed_ms wait_ms timeout_ms count event_count server_request_count sequence revision queue_depth body_bytes output_bytes http_status transport_operation_kind surface generation accepted").split(" "));

/** Profiler copies only correlation metadata, never raw diagnostics or protocol payloads. */
export function safeRecord(value, source, cutoff) {
  const timestamp = value.timestamp_ms ?? Date.parse(value.timestamp);
  if (!Number.isFinite(timestamp) || timestamp < cutoff || !/^[a-z][a-z0-9_]{0,119}$/.test(value.event ?? "")) return undefined;
  return {
    timestamp_ms: timestamp, source, event: value.event,
    fields: Object.fromEntries(Object.entries(value.fields ?? {}).filter(([key, item]) =>
      fields.has(key) && (typeof item === "number" && Number.isFinite(item) || typeof item === "boolean"
        || typeof item === "string" && /^[a-zA-Z0-9_.:-]{1,160}$/.test(item) && !item.includes("@")),
    )),
  };
}

/** A segment overlapping the window is kept; readers filter individual record timestamps. */
export function expiredSegment(name, now, options = defaults) {
  const match = /^(\d+)\.(jsonl|cpuprofile)$/.exec(name);
  return Boolean(match && Number(match[1]) + options.segmentMs < now - options.windowMs);
}

export function parseProcessStat(text) {
  const values = text.slice(text.lastIndexOf(")") + 2).trim().split(/\s+/);
  return { state: values[0], cpu_ticks: Number(values[11]) + Number(values[12]), start_ticks: values[19], rss_pages: Number(values[21]) };
}

function classified(error) { return /^[A-Z][A-Z0-9_]+$/.test(error?.code ?? "") ? error.code : "operation_failed"; }
function event(name, details = {}) {
  console.log(JSON.stringify({ timestamp_ms: Date.now(), event: name, fields: { attempt: 1, ...details } }));
}

class LogTail {
  offset = 0;
  inode;
  partial = "";
  constructor(file, source) { this.file = file; this.source = source; }
  async read(cutoff) {
    const handle = await open(this.file, "r");
    try {
      const info = await handle.stat();
      if (this.inode !== info.ino || info.size < this.offset) {
        // Bootstrap and rotation use a bounded tail. Truncated history is explicit.
        this.offset = Math.max(0, info.size - maxReadBytes);
        this.partial = "";
        if (this.offset) event("profiler_log_history_truncated", { source: this.source, skipped_bytes: this.offset });
        this.inode = info.ino;
        if (this.offset) this.partial = null;
      }
      const length = Math.min(maxReadBytes, info.size - this.offset);
      const buffer = Buffer.alloc(length);
      const { bytesRead } = await handle.read(buffer, 0, length, this.offset);
      this.offset += bytesRead;
      let text = buffer.subarray(0, bytesRead).toString("utf8");
      if (this.partial === null) {
        const newline = text.indexOf("\n");
        if (newline < 0) return [];
        text = text.slice(newline + 1);
      } else text = this.partial + text;
      const lines = text.split("\n");
      this.partial = lines.pop();
      if (this.partial.length > maxReadBytes) { this.partial = null; event("profiler_log_record_too_large", { source: this.source }); }
      return lines.flatMap(line => {
        try { const record = safeRecord(JSON.parse(line), this.source, cutoff); return record ? [record] : []; }
        catch { return []; } // Plain shell output is deliberately excluded.
      });
    } finally { await handle.close(); }
  }
}

async function processSample(pid, previous, ticksPerSecond, pageSize) {
  const started = performance.now();
  const base = `/proc/${pid}`;
  const info = parseProcessStat(await readFile(`${base}/stat`, "utf8"));
  const identity = `${pid}:${info.start_ticks}`;
  const last = previous.get(identity);
  previous.set(identity, { ticks: info.cpu_ticks, time: started });
  const result = {
    pid: Number(pid), process_instance: identity, state: info.state,
    cpu_percent: last ? Math.round((info.cpu_ticks - last.ticks) / ticksPerSecond / ((started - last.time) / 1000) * 1000) / 10 : null,
    rss_bytes: info.rss_pages * pageSize,
  };
  const io = await readFile(`${base}/io`, "utf8").catch(() => "");
  for (const key of ["read_bytes", "write_bytes", "rchar", "wchar"]) {
    const match = new RegExp(`^${key}: (\\d+)$`, "m").exec(io);
    if (match) result[key] = Number(match[1]);
  }
  // Thread state and wait channel distinguish CPU work from sleeping/lock waits.
  result.threads = (await Promise.all((await readdir(`${base}/task`)).map(async tid => {
    try {
      const thread = parseProcessStat(await readFile(`${base}/task/${tid}/stat`, "utf8"));
      const wait = (await readFile(`${base}/task/${tid}/wchan`, "utf8")).trim();
      return { tid: Number(tid), state: thread.state, cpu_ticks: thread.cpu_ticks, wait_kind: /^[a-zA-Z0-9_]+$/.test(wait) ? wait : "unknown" };
    } catch { return undefined; } // A thread can exit during a sample.
  }))).filter(Boolean);
  return result;
}

class Inspector {
  pending = new Map();
  nextId = 1;
  async connect(pid) {
    // Enable only the Web Shell's inspector; never agent runtimes or arbitrary Node processes.
    process.kill(pid, "SIGUSR1");
    let target;
    for (let attempt = 0; attempt < 20; attempt++) {
      try {
        const response = await fetch("http://127.0.0.1:9229/json/list", { signal: AbortSignal.timeout(1000) });
        [target] = await response.json();
        if (target) break;
      } catch {}
      await delay(100);
    }
    if (!target?.webSocketDebuggerUrl?.startsWith("ws://127.0.0.1:9229/")) throw new Error("inspector_unavailable");
    this.socket = new WebSocket(target.webSocketDebuggerUrl);
    await new Promise((resolve, reject) => {
      const timeout = setTimeout(() => reject(new Error("inspector_timeout")), 5000);
      this.socket.onopen = () => { clearTimeout(timeout); resolve(); };
      this.socket.onerror = () => { clearTimeout(timeout); reject(new Error("inspector_unavailable")); };
    });
    this.socket.onmessage = message => {
      const reply = JSON.parse(message.data);
      const callback = this.pending.get(reply.id);
      if (callback) { this.pending.delete(reply.id); callback(reply); }
    };
    const { result } = await this.send("Runtime.evaluate", { expression: "process.pid", returnByValue: true });
    if (result.value !== pid) { this.close(); throw new Error("inspector_pid_mismatch"); }
    await this.send("Profiler.enable");
    await this.send("Profiler.setSamplingInterval", { interval: 10_000 });
    await this.send("Profiler.start");
    this.pid = pid;
    this.startedAt = Date.now();
  }
  send(method, params = {}) {
    return new Promise((resolve, reject) => {
      const id = this.nextId++;
      const timer = setTimeout(() => { this.pending.delete(id); reject(new Error("inspector_timeout")); }, 5000);
      this.pending.set(id, reply => { clearTimeout(timer); reply.error ? reject(new Error("inspector_failed")) : resolve(reply.result); });
      this.socket.send(JSON.stringify({ id, method, params }));
    });
  }
  close() { this.socket?.close(); }
  async finish(root, restart = true) {
    const startedAt = this.startedAt;
    const { profile } = await this.send("Profiler.stop");
    // Code symbols aid diagnosis; raw source paths/URLs are excluded from stored profiles.
    for (const node of profile.nodes) node.callFrame.url = "";
    profile.captureStartedAt = this.startedAt;
    profile.captureEndedAt = Date.now();
    profile.pid = this.pid;
    await writeFile(path.join(root, `${this.startedAt}.cpuprofile`), JSON.stringify(profile), { mode: 0o600 });
    event("profiler_cpu_capture_completed", { operation_id: `cpu-${startedAt}`, outcome: "success", duration_ms: profile.captureEndedAt - startedAt, sample_count: profile.samples?.length ?? 0 });
    if (restart) {
      await this.send("Profiler.start");
      this.startedAt = Date.now();
      event("profiler_cpu_capture_started", { operation_id: `cpu-${this.startedAt}`, pid: this.pid });
    }
  }
}

/** Standalone service owns capture and retention; product processes need no restart. */
export async function run(options = defaults) {
  await mkdir(options.root, { recursive: true, mode: 0o700 });
  await chmod(options.root, 0o700);
  const tails = options.logs.map((file, index) => new LogTail(file, index === 0 ? "app_server" : "web_shell"));
  const previous = new Map();
  const ticksPerSecond = Number(execFileSync("getconf", ["CLK_TCK"], { encoding: "utf8" }).trim());
  const pageSize = Number(execFileSync("getconf", ["PAGESIZE"], { encoding: "utf8" }).trim());
  let inspector;
  let segment;
  let bytes = 0;
  let capped = false;
  let stopping = false;
  let lastCpuFlush = 0;
  let nextInspectorAttempt = 0;
  process.once("SIGTERM", () => { stopping = true; });
  process.once("SIGINT", () => { stopping = true; });
  event("driver_profiler_started", { window_ms: options.windowMs, cpu_sampling_interval_us: 10_000 });
  while (!stopping) {
    const started = Date.now();
    const operationId = `capture-${started}`;
    try {
      if (!segment || started - segment >= options.segmentMs) {
        if (inspector) {
          try { await inspector.finish(options.root); }
          catch (error) { inspector.close(); inspector = undefined; event("profiler_cpu_capture_failed", { operation_id: operationId, outcome: "failure", error_kind: classified(error) }); }
        }
        segment = started; bytes = 0; capped = false;
        for (const name of await readdir(options.root)) {
          if (expiredSegment(name, started, options)) await rm(path.join(options.root, name));
        }
        event("profiler_capture_started", { operation_id: operationId });
      }
      const cgroup = execFileSync("systemctl", ["--user", "show", options.unit, "-p", "ControlGroup", "--value"], { encoding: "utf8", timeout: 2000 }).trim();
      if (!cgroup.startsWith("/user.slice/")) throw new Error("driver_unavailable");
      const pids = (await readFile(`/sys/fs/cgroup${cgroup}/cgroup.procs`, "utf8")).trim().split(/\s+/).filter(pid => /^\d+$/.test(pid));
      const processes = [];
      let webPid;
      for (const pid of pids) {
        try {
          const sample = await processSample(pid, previous, ticksPerSecond, pageSize);
          const command = (await readFile(`/proc/${pid}/cmdline`, "utf8")).split("\0");
          sample.role = command.some(arg => arg === "src/dev-server.mjs") ? "web_shell"
            : command[0]?.endsWith("/openaide-app-server") ? "app_server" : "driver_child";
          if (sample.role === "web_shell") webPid = Number(pid);
          processes.push(sample);
        } catch (error) { if (!["ENOENT", "ESRCH"].includes(error.code)) throw error; }
      }
      const active = new Set(processes.map(item => item.process_instance));
      for (const key of previous.keys()) if (!active.has(key)) previous.delete(key);
      if (inspector && inspector.pid !== webPid) { inspector.close(); inspector = undefined; }
      if (!inspector && webPid && started >= nextInspectorAttempt) {
        event("profiler_cpu_attach_started", { operation_id: operationId, pid: webPid });
        const candidate = new Inspector();
        try {
          await candidate.connect(webPid); inspector = candidate;
          event("profiler_cpu_attach_completed", { operation_id: operationId, pid: webPid, outcome: "success", duration_ms: Date.now() - started });
          event("profiler_cpu_capture_started", { operation_id: `cpu-${candidate.startedAt}`, pid: webPid });
        } catch (error) {
          candidate.close(); nextInspectorAttempt = Date.now() + 60_000;
          event("profiler_cpu_attach_completed", { operation_id: operationId, outcome: "failure", error_kind: classified(error), retry_delay_ms: 60_000, duration_ms: Date.now() - started });
        }
      }
      // A durable request also works during startup, before signal handlers exist.
      const requestedFlush = Number(await readFile(path.join(options.root, "flush-request.json"), "utf8").catch(() => "0"));
      if (requestedFlush > lastCpuFlush) {
        if (inspector) await inspector.finish(options.root);
        lastCpuFlush = Date.now();
      }
      const records = [{ timestamp_ms: started, source: "resources", processes,
        load_average: (await readFile("/proc/loadavg", "utf8")).trim().split(" ").slice(0, 3).map(Number),
        memory_pressure: await readFile("/proc/pressure/memory", "utf8").catch(() => "unavailable"),
        cpu_pressure: await readFile("/proc/pressure/cpu", "utf8").catch(() => "unavailable"),
        io_pressure: await readFile("/proc/pressure/io", "utf8").catch(() => "unavailable"),
      }];
      for (const tail of tails) {
        try { records.push(...await tail.read(started - options.windowMs)); }
        catch (error) { event("profiler_log_read_failed", { source: tail.source, outcome: "failure", error_kind: classified(error) }); }
      }
      const data = records.map(record => JSON.stringify(record) + "\n").join("");
      if (bytes + Buffer.byteLength(data) <= maxSegmentBytes) {
        await appendFile(path.join(options.root, `${segment}.jsonl`), data, { mode: 0o600 });
        bytes += Buffer.byteLength(data);
      } else if (!capped) {
        capped = true; event("profiler_segment_capacity_reached", { outcome: "partial", dropped_records: records.length });
      }
      const status = { timestamp_ms: Date.now(), window_ms: options.windowMs, segment_ms: options.segmentMs,
        processes: processes.length, web_cpu_profiles: Boolean(inspector), native_stack_profiles: false, browser_profiles: false,
        last_cpu_flush_ms: lastCpuFlush, segment_capped: capped, outcome: "success", duration_ms: Date.now() - started };
      await writeFile(path.join(options.root, "status.tmp"), JSON.stringify(status), { mode: 0o600 });
      await rename(path.join(options.root, "status.tmp"), path.join(options.root, "status.json"));
      if (started === segment) event("profiler_capture_completed", { operation_id: operationId, outcome: "success", duration_ms: Date.now() - started, process_count: processes.length });
    } catch (error) {
      event("profiler_capture_completed", { operation_id: operationId, outcome: "failure", duration_ms: Date.now() - started, error_kind: classified(error) });
      // Let systemd restart a failed collector rather than silently preserving stale status.
      inspector?.close(); throw error;
    }
    await delay(Math.max(0, 1000 - (Date.now() - started)));
  }
  if (inspector) { await inspector.finish(options.root, false).catch(() => {}); inspector.close(); }
  event("driver_profiler_stopped", { outcome: "success" });
}

/** Freeze evidence before diagnosis so rolling retention cannot erase it mid-investigation. */
export async function snapshot(options = defaults) {
  let flushOutcome = "not_requested";
  if (options.flush) {
    const requestedAt = Date.now();
    try {
      const requestFile = path.join(options.root, `flush-request-${process.pid}.tmp`);
      await writeFile(requestFile, String(requestedAt), { mode: 0o600 });
      await rename(requestFile, path.join(options.root, "flush-request.json"));
      flushOutcome = "timeout";
      for (let attempt = 0; attempt < 40; attempt++) {
        const status = JSON.parse(await readFile(path.join(options.root, "status.json"), "utf8"));
        if (status.last_cpu_flush_ms >= requestedAt) { flushOutcome = status.web_cpu_profiles ? "success" : "unavailable"; break; }
        await delay(200);
      }
    } catch { flushOutcome = "unavailable"; }
  }
  const now = Date.now();
  const root = path.join(path.dirname(options.root), "driver-profiler-snapshots", String(now));
  await mkdir(root, { recursive: true, mode: 0o700 });
  for (const name of await readdir(options.root)) {
    if (!/^(\d+)\.(jsonl|cpuprofile)$/.test(name) || expiredSegment(name, now, options)) continue;
    try {
      if (name.endsWith(".jsonl")) {
        const lines = (await readFile(path.join(options.root, name), "utf8")).split("\n").flatMap(line => {
          try { return JSON.parse(line).timestamp_ms >= now - options.windowMs ? [line] : []; } catch { return []; }
        });
        await writeFile(path.join(root, name), lines.join("\n") + "\n", { mode: 0o600 });
      } else { await copyFile(path.join(options.root, name), path.join(root, name)); await chmod(path.join(root, name), 0o600); }
    } catch (error) { if (error.code !== "ENOENT") throw error; }
  }
  const status = JSON.parse(await readFile(path.join(options.root, "status.json"), "utf8"));
  await writeFile(path.join(root, "manifest.json"), JSON.stringify({ ...status, cpu_flush_outcome: flushOutcome, captured_at_ms: now, cutoff_ms: now - options.windowMs }), { mode: 0o600 });
  console.log(root);
  return root;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const command = process.argv[2] ?? "run";
  try {
    if (command === "snapshot") await snapshot();
    else if (command === "status") console.log(await readFile(path.join(defaults.root, "status.json"), "utf8"));
    else if (command === "run") await run();
    else throw new Error("Expected run, snapshot, or status");
  } catch (error) { event("driver_profiler_failed", { outcome: "failure", error_kind: classified(error) }); process.exitCode = 1; }
}
