import { spawn } from "node:child_process";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

const [binaryPath, workspaceRoot] = process.argv.slice(2);
if (!binaryPath || !workspaceRoot) {
  throw new Error("Usage: node scripts/smoke-packaged-codex-acp.mjs <app-server> <workspace>");
}

const stateParent = await mkdtemp(path.join(os.tmpdir(), "openaide-codex-acp-smoke-"));
const stateRoot = path.join(stateParent, "state");
const adapterLogs = path.join(stateParent, "adapter-logs");
await mkdir(path.join(stateRoot, "agents"), { recursive: true });
// Releases must recover catalogs written before built-in ids were reserved.
// The missing command makes any accidental Custom-Agent shadowing fail here.
await writeFile(path.join(stateRoot, "agents", "catalog.json"), JSON.stringify({
  schemaVersion: 1,
  records: [{
    id: "codex",
    label: "Codex",
    source_kind: "custom",
    transport: "stdio",
    command: "missing-global-codex-acp",
  }],
}));
const child = spawn(path.resolve(binaryPath), [], {
  env: {
    ...process.env,
    OPENAIDE_APP_SERVER_PROTOCOL: "app-server-protocol",
    OPENAIDE_STORAGE_ROOT: stateRoot,
    APP_SERVER_LOGS: adapterLogs,
  },
  stdio: "pipe",
  windowsHide: true,
});

let stderr = "";
let stdoutBuffer = "";
let nextRequestId = 1;
const pending = new Map();
const closed = new Promise((resolve) => {
  child.once("close", (code, signal) => {
    const error = new Error(`App Server exited before the smoke completed (code=${code}, signal=${signal})`);
    for (const waiter of pending.values()) waiter.reject(error);
    pending.clear();
    resolve();
  });
});

child.stderr.setEncoding("utf8").on("data", (chunk) => { stderr += chunk; });
child.once("error", (error) => {
  for (const waiter of pending.values()) waiter.reject(error);
  pending.clear();
});
child.stdin.on("error", () => {});
child.stdout.setEncoding("utf8").on("data", (chunk) => {
  stdoutBuffer += chunk;
  for (;;) {
    const newline = stdoutBuffer.indexOf("\n");
    if (newline < 0) break;
    const line = stdoutBuffer.slice(0, newline).trim();
    stdoutBuffer = stdoutBuffer.slice(newline + 1);
    if (!line) continue;
    let message;
    try { message = JSON.parse(line); } catch { continue; }
    const waiter = pending.get(String(message.id));
    if (!waiter) continue;
    pending.delete(String(message.id));
    if (message.error) {
      const rpcError = new Error(message.error?.error?.message ?? "App Server request failed");
      rpcError.code = message.error?.error?.code;
      waiter.reject(rpcError);
    } else waiter.resolve(message.result);
  }
});

function request(method, params, timeoutMs = 120_000) {
  const id = String(nextRequestId++);
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`${method} timed out after ${timeoutMs}ms`));
    }, timeoutMs);
    pending.set(id, {
      resolve: (value) => { clearTimeout(timeout); resolve(value); },
      reject: (error) => { clearTimeout(timeout); reject(error); },
    });
    child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
  });
}

try {
  await request("client/initialize", {
    clientInstanceId: "packaged-codex-acp-smoke",
    shell: { kind: "desktop" },
    requestedSurface: { kind: "home" },
    workspaceRoots: [],
  }, 30_000);
  const added = await request("project/add", { workspaceRoot: path.resolve(workspaceRoot) }, 30_000);
  const project = added?.result?.project;
  if (!project?.projectId) {
    throw new Error("App Server did not add the smoke-test project");
  }

  const acquiredEnvelope = await request("task/acquire", {
    projectId: project.projectId,
    agentId: "codex",
  }, 30_000);
  const acquired = acquiredEnvelope?.result?.task;
  if (!acquired?.task?.taskId) {
    throw new Error("App Server did not persist the packaged smoke-test Task");
  }

  try {
    const listedEnvelope = await request("agent/listSessions", {
      agentId: "codex",
      projectId: project.projectId,
    });
    const listed = listedEnvelope?.result;
    if (listed?.agentId !== "codex" || !Array.isArray(listed.sessions)) {
      throw new Error("Unexpected Codex session-list response shape");
    }
    console.log("Verified packaged App Server Task persistence, Codex ACP initialization, and session listing.");
  } catch (error) {
    // An unauthenticated release runner cannot list private Codex sessions, but
    // this response proves the adapter initialized and survived the ACP request.
    if (error.code !== "unauthorized") throw error;
    console.log("Verified packaged App Server Task persistence and Codex ACP initialization through the authentication boundary.");
  }
} catch (error) {
  const adapterLog = await readFile(path.join(adapterLogs, "app-server.log"), "utf8")
    .catch(() => "");
  const diagnosticLines = adapterLog
    .split(/\r?\n/u)
    .filter((line) => line.includes("[ERR]") || line.includes("[SYSTEM_ERROR]"))
    .join("\n")
    .replaceAll(stateParent, "<smoke-state>")
    .replaceAll(path.resolve(workspaceRoot), "<workspace>");
  throw new Error(
    `${error.message}; App Server stderr: ${stderr.slice(0, 2_000)}; `
      + `Codex adapter errors: ${diagnosticLines.slice(-4_000)}`,
  );
} finally {
  if (!child.stdin.destroyed) child.stdin.end();
  await Promise.race([
    closed,
    new Promise((resolve) => setTimeout(resolve, 10_000)),
  ]);
  if (child.exitCode === null) child.kill();
  await rm(stateParent, { recursive: true, force: true });
}
