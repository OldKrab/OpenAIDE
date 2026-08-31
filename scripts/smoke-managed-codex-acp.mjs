import { spawn } from "node:child_process";
import { access } from "node:fs/promises";
import path from "node:path";

const [runtimeRoot] = process.argv.slice(2);
if (!runtimeRoot) {
  throw new Error("Usage: node scripts/smoke-managed-codex-acp.mjs <runtime-root>");
}
if (process.platform !== "win32" || process.arch !== "x64") {
  throw new Error("The managed Windows Codex smoke requires Windows x64");
}

const adapterPath = path.resolve(
  runtimeRoot,
  "node_modules/@openaide/codex-acp/dist/index.js",
);
const codexPath = path.resolve(
  runtimeRoot,
  "node_modules/@openai/codex-win32-x64/vendor/x86_64-pc-windows-msvc/bin/codex.exe",
);
await Promise.all([access(adapterPath), access(codexPath)]);

const child = spawn(process.execPath, [adapterPath], {
  env: {
    ...process.env,
    CODEX_PATH: codexPath,
    NO_BROWSER: "1",
  },
  stdio: "pipe",
  windowsHide: true,
});

let stderr = "";
let stdoutBuffer = "";
let exitDescription;
const pending = new Map();
child.stderr.setEncoding("utf8").on("data", (chunk) => { stderr += chunk; });
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
    waiter.resolve(message);
  }
});

const closed = new Promise((resolve) => {
  child.once("close", (code, signal) => {
    exitDescription = `code=${code}, signal=${signal}`;
    for (const waiter of pending.values()) {
      waiter.reject(new Error(`Codex ACP exited before responding (${exitDescription})`));
    }
    pending.clear();
    resolve();
  });
});
child.once("error", (error) => {
  for (const waiter of pending.values()) waiter.reject(error);
  pending.clear();
});
child.stdin.on("error", () => {});

let nextRequestId = 1;
function request(method, params, timeoutMs = 20_000) {
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
  const initialized = await request("initialize", {
    protocolVersion: 1,
    clientCapabilities: {},
  });
  if (initialized.error || initialized.result?.protocolVersion !== 1) {
    throw new Error(`Codex ACP initialization failed: ${JSON.stringify(initialized.error)}`);
  }

  // Authentication may legitimately reject listing on a clean runner. The
  // release invariant is that the managed native Codex process stays alive and
  // returns an ACP response instead of disappearing at this boundary.
  await request("session/list", { cwd: process.cwd() });
  if (exitDescription) {
    throw new Error(`Codex ACP exited during session listing (${exitDescription})`);
  }
  console.log("Verified the managed Windows Codex ACP process through session listing.");
} catch (error) {
  throw new Error(`${error.message}; Codex ACP stderr: ${stderr.slice(0, 4_000)}`);
} finally {
  if (!child.stdin.destroyed) child.stdin.end();
  await Promise.race([
    closed,
    new Promise((resolve) => setTimeout(resolve, 2_000)),
  ]);
  if (child.exitCode === null) child.kill();
}
