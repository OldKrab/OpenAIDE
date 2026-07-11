import { spawn, spawnSync } from "node:child_process";
import { closeSync, existsSync, mkdirSync, openSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const webPort = Number(process.env.OPENAIDE_WEB_PORT ?? "5474");
const vitePort = Number(process.env.OPENAIDE_WEB_VITE_PORT ?? "5473");
const stateRoot = process.env.OPENAIDE_WEB_STATE_ROOT ?? path.join(repoRoot, ".openaide-web-dev-single", "state");
const runtimeRoot = process.env.OPENAIDE_WEB_RUNTIME_ROOT ?? path.join(repoRoot, ".openaide-web-dev-single", "runtime");
const logDir = path.join(repoRoot, ".openaide-web-dev-single", "logs");
const logPath = path.join(logDir, "web.log");
const allowedHosts = configuredAllowedHosts();

mkdirSync(logDir, { recursive: true });
mkdirSync(stateRoot, { recursive: true });
mkdirSync(runtimeRoot, { recursive: true });

run("npm", ["run", "app-server:build"]);
buildWebDependencies();
const killed = stopExistingWebApps();
writeFileSync(logPath, "");
const logFd = openSync(logPath, "a");

const child = spawn(
  "npm",
  ["run", "dev", "--workspace", "openaide-web"],
  {
    cwd: repoRoot,
    detached: true,
    env: {
      ...webShellEnv(),
      OPENAIDE_WEB_PORT: String(webPort),
      OPENAIDE_WEB_VITE_PORT: String(vitePort),
      OPENAIDE_WEB_ALLOWED_HOSTS: allowedHosts,
      OPENAIDE_WEB_STATE_ROOT: stateRoot,
      OPENAIDE_WEB_RUNTIME_ROOT: runtimeRoot,
    },
    stdio: ["ignore", logFd, logFd],
  },
);
child.unref();
closeSync(logFd);

console.log(`Stopped ${killed.length} existing web process${killed.length === 1 ? "" : "es"}.`);
console.log(`Started OpenAIDE web launcher PID ${child.pid}.`);
console.log(`Web shell: http://127.0.0.1:${webPort}/`);
console.log(`Vite: http://127.0.0.1:${vitePort}/`);
console.log(`Log: ${path.relative(repoRoot, logPath)}`);
if (allowedHosts) console.log(`Allowed hosts: ${allowedHosts}`);
else console.log("Allowed hosts: local only");

function buildWebDependencies() {
  run("npm", ["run", "build", "--workspace", "@openaide/app-server-client"]);
  run("npm", ["run", "build", "--workspace", "@openaide/app-shell-contracts"]);
}

function run(command, args) {
  const result = spawnSync(command, args, { cwd: repoRoot, encoding: "utf8", stdio: "inherit" });
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

function webShellEnv() {
  const {
    OPENAIDE_PROJECT_ROOTS: _projectRoots,
    OPENAIDE_WEB_PROJECT_ROOTS: _webProjectRoots,
    ...env
  } = process.env;
  return env;
}

function stopExistingWebApps() {
  const processRows = listProcesses();
  const listenerPids = new Set([
    ...listListenerPids(webPort),
    ...listListenerPids(vitePort),
  ]);
  const matchedPids = new Set();
  for (const row of processRows.values()) {
    if (listenerPids.has(row.pid) || isOpenAideWebProcess(row.command)) {
      matchedPids.add(row.pid);
    }
  }
  const allTargets = descendantsOf(processRows, matchedPids);
  const safeTargets = [...allTargets].filter((pid) => pid !== process.pid && pid !== process.ppid);
  if (safeTargets.length === 0) return [];
  killPids(safeTargets, "SIGTERM");
  sleep(1_000);
  const remaining = safeTargets.filter((pid) => processExists(pid));
  if (remaining.length > 0) {
    killPids(remaining, "SIGKILL");
  }
  return safeTargets;
}

function listProcesses() {
  const result = spawnSync("ps", ["-eo", "pid=,ppid=,command="], { encoding: "utf8" });
  if (result.status !== 0) return new Map();
  const rows = new Map();
  for (const line of result.stdout.split("\n")) {
    const match = /^\s*(\d+)\s+(\d+)\s+(.+)$/.exec(line);
    if (!match) continue;
    const pid = Number(match[1]);
    rows.set(pid, { pid, ppid: Number(match[2]), command: match[3] });
  }
  return rows;
}

function listListenerPids(port) {
  const result = spawnSync("ss", ["-ltnp"], { encoding: "utf8" });
  if (result.status !== 0) return [];
  const pids = [];
  for (const line of result.stdout.split("\n")) {
    if (!line.includes(`:${port}`)) continue;
    for (const match of line.matchAll(/pid=(\d+)/g)) {
      pids.push(Number(match[1]));
    }
  }
  return pids;
}

function isOpenAideWebProcess(command) {
  return command.includes("npm run dev --workspace openaide-web")
    || (command.includes("vite") && command.includes("--host 127.0.0.1") && command.includes(`--port ${vitePort}`));
}

function descendantsOf(processRows, roots) {
  const targets = new Set(roots);
  let changed = true;
  while (changed) {
    changed = false;
    for (const row of processRows.values()) {
      if (targets.has(row.ppid) && !targets.has(row.pid)) {
        targets.add(row.pid);
        changed = true;
      }
    }
  }
  return targets;
}

function killPids(pids, signal) {
  for (const pid of pids) {
    try {
      process.kill(pid, signal);
    } catch {
      // Process may have exited between discovery and shutdown.
    }
  }
}

function processExists(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

function sleep(ms) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

function configuredAllowedHosts() {
  if (process.env.OPENAIDE_WEB_ALLOWED_HOSTS !== undefined) {
    return process.env.OPENAIDE_WEB_ALLOWED_HOSTS;
  }
  const hostsFile = process.env.OPENAIDE_WEB_ALLOWED_HOSTS_FILE
    ?? path.join(repoRoot, ".openaide-web-dev-single", "allowed-hosts");
  if (!existsSync(hostsFile)) return "";
  return readFileSync(hostsFile, "utf8")
    .split(/\r?\n/)
    .map((line) => line.split("#")[0]?.trim() ?? "")
    .filter(Boolean)
    .join(",");
}
