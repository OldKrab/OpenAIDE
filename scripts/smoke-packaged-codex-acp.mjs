import { spawn } from "node:child_process";
import { mkdir, mkdtemp, readdir, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { smokeCodexSessionRecovery } from "./smoke-codex-session-recovery.mjs";
import { smokeCodexNativeRecovery } from "./smoke-codex-native-recovery.mjs";

const [binaryPath, workspaceRoot] = process.argv.slice(2);
if (!binaryPath || !workspaceRoot) {
  throw new Error("Usage: node scripts/smoke-packaged-codex-acp.mjs <app-server> <workspace>");
}

const stateParent = await mkdtemp(path.join(os.tmpdir(), "openaide-codex-acp-smoke-"));
const stateRoot = path.join(stateParent, "state");
const codexHome = path.join(stateParent, "codex");
const nativeConfig = {
  cli_auth_credentials_store: "file",
  log_dir: path.join(stateParent, "native-logs"),
};
await mkdir(codexHome);
await writeFile(path.join(codexHome, "config.toml"),
  `cli_auth_credentials_store = "file"\nlog_dir = ${JSON.stringify(nativeConfig.log_dir)}\n`);
// Task acquisition starts native preparation in the background. Its failure
// handler can log out, so even this authentication-boundary smoke must never
// borrow the caller's credentials, native database, configuration, or logs.
const childEnv = {
  ...process.env,
  CODEX_HOME: codexHome,
  CODEX_SQLITE_HOME: path.join(stateParent, "native-state"),
  CODEX_CONFIG: JSON.stringify(nativeConfig),
  APP_SERVER_LOGS: path.join(stateParent, "adapter-logs"),
  OPENAIDE_APP_SERVER_PROTOCOL: "app-server-protocol",
  OPENAIDE_STORAGE_ROOT: stateRoot,
};
for (const name of ["CODEX_PATH", "DEFAULT_AUTH_REQUEST", "MODEL_PROVIDER", "INITIAL_AGENT_MODE", "CODEX_API_KEY", "OPENAI_API_KEY", "CODEX_ACCESS_TOKEN"]) {
  delete childEnv[name];
}
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
  cwd: stateParent,
  env: childEnv,
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

async function waitForClose(timeoutMs) {
  let timer;
  try {
    return await Promise.race([
      closed.then(() => true),
      new Promise((resolve) => { timer = setTimeout(() => resolve(false), timeoutMs); }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}

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
      const rpcError = new Error(`${waiter.method}: ${message.error?.error?.message ?? "App Server request failed"}`);
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
      method,
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
  await request("agent/probe", { agentId: "codex" });
  const detailsEnvelope = await request("settings/getAgentDetails", {});
  const codexDetails = detailsEnvelope?.result?.agents?.find((agent) => agent.agentId === "codex");
  const authMethodIds = codexDetails?.authMethods?.map((method) => method.id) ?? [];
  if (!authMethodIds.includes("chat-gpt")) {
    throw new Error("Packaged Codex did not advertise ChatGPT browser authentication");
  }
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

  // The release/bootstrap path already provisions the locked adapter. Reuse
  // those installed bytes so this wire regression needs no unit-CI download.
  if (process.platform !== "win32") {
    const runtimesRoot = path.join(stateRoot, "agent-runtimes", "codex-acp");
    const runtimes = (await readdir(runtimesRoot, { withFileTypes: true }))
      .filter((entry) => entry.isDirectory() && !entry.name.startsWith("."));
    if (runtimes.length !== 1) throw new Error("Expected one freshly provisioned Codex runtime");
    const modules = path.join(runtimesRoot, runtimes[0].name, "node_modules");
    const adapter = path.join(modules, "@openaide", "codex-acp", "dist", "index.js");
    await smokeCodexSessionRecovery(adapter);
    // The managed package's launcher selects its pinned platform binary. This
    // covers native policy persistence as well as the adapter's wire requests.
    await smokeCodexNativeRecovery(path.join(modules, "@openai", "codex", "bin", "codex.js"), adapter);
  } else {
    // The native fixture is a POSIX executable. Windows keeps the real native
    // bootstrap checks above until this additional fixture has a tested launcher.
    console.log("Skipped recovery wire fixture on Windows: POSIX fixture launcher required.");
  }
} catch (error) {
  throw new Error(`${error.message}; App Server stderr: ${stderr.slice(0, 2_000)}`);
} finally {
  // EOF can let the Windows parent exit before its native descendants release
  // inherited pipes and the temporary cwd. Terminate the owned tree while its
  // parent is still addressable; taskkill cannot find a tree after parent exit.
  if (process.platform === "win32" && child.exitCode === null && child.signalCode === null) {
    await new Promise((resolve, reject) => {
      const killer = spawn("taskkill", ["/PID", String(child.pid), "/T", "/F"], {
        stdio: "ignore", windowsHide: true, timeout: 5_000,
      });
      killer.once("error", reject);
      // A concurrent exit may produce a nonzero result; observe closure below.
      killer.once("close", resolve);
    });
  } else if (!child.stdin.destroyed) child.stdin.end();
  if (!await waitForClose(10_000)) {
    if (process.platform !== "win32" && child.exitCode === null && child.signalCode === null) {
      child.kill("SIGKILL");
    }
    if (!await waitForClose(5_000)) {
      throw new Error(`App Server shutdown timed out (exit=${child.exitCode}, signal=${child.signalCode}); temporary smoke state was retained`);
    }
  }
  await rm(stateParent, { recursive: true, force: true });
}
