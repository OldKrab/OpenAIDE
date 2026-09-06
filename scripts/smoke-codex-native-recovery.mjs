import assert from "node:assert/strict";
import { spawn, execFile } from "node:child_process";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import os from "node:os";
import path from "node:path";

const execute = promisify(execFile);

/** Actual adapter/native restart coverage; its isolated provider never contacts a model. */
export async function smokeCodexNativeRecovery(binary, adapter) {
  for (const scenario of ["readOnly", "workspace", "worktree", "named-extra-root"]) {
    try {
      await verifyRecovery(binary, adapter, scenario);
    } catch (error) {
      throw new Error(`native_recovery_failed: scenario=${scenario}; ${error.message}`);
    }
  }
}

async function verifyRecovery(binary, adapter, scenario) {
  const root = await mkdtemp(path.join(os.tmpdir(), "openaide-native-recovery-"));
  const home = path.join(root, "home");
  let workspace = path.join(root, "workspace");
  await mkdir(home);
  await mkdir(workspace);
  const extraRoot = path.join(root, "extra");
  if (scenario === "named-extra-root") await mkdir(extraRoot);
  if (scenario === "worktree") {
    const gitConfig = path.join(root, "gitconfig");
    const hooks = path.join(root, "empty-hooks");
    await writeFile(gitConfig, "");
    await mkdir(hooks);
    const gitEnv = { ...process.env };
    // Inherited GIT_DIR/WORK_TREE or injected configuration must not redirect
    // these fixture-only mutations into a developer's repository.
    for (const key of Object.keys(gitEnv)) if (key.startsWith("GIT_")) delete gitEnv[key];
    Object.assign(gitEnv, { GIT_CONFIG_NOSYSTEM: "1", GIT_CONFIG_GLOBAL: gitConfig, GIT_CONFIG_COUNT: "0" });
    const git = (...args) => execute("git", ["-c", `core.hooksPath=${hooks}`,
      "-c", "commit.gpgsign=false", ...args], {
      cwd: workspace, timeout: 10000,
      env: gitEnv,
    });
    await git("init", "--quiet");
    await git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "commit", "--quiet", "--allow-empty", "-m", "Fixture");
    const worktree = path.join(root, "worktree");
    await git("worktree", "add", "--quiet", "-b", "fixture-worktree", worktree);
    workspace = worktree;
  }
  await writeFile(path.join(home, "config.toml"), [
    'model = "gpt-5.4"', 'model_provider = "fixture"', 'approval_policy = "never"',
    'cli_auth_credentials_store = "file"',
    'sandbox_mode = "danger-full-access"', '[model_providers.fixture]',
    'name = "Offline recovery fixture"', 'base_url = "http://127.0.0.1:9/v1"',
    'wire_api = "responses"', 'request_max_retries = 0', 'stream_max_retries = 0',
    'supports_websockets = false', '',
  ].join("\n"));
  let client;
  try {
    client = await rpcClient(binary, home);
    const initial = await client.request("thread/start", { cwd: workspace,
      ...(scenario === "named-extra-root" && {
        runtimeWorkspaceRoots: [workspace, extraRoot], permissions: "fixtureSaved",
        config: { permissions: { fixtureSaved: { filesystem: {
          ":root": "read", ":workspace_roots": { ".": "write", denied: "deny" },
        }, network: { enabled: false } } } },
      }),
    });
    const threadId = initial.thread.id;
    await client.request("thread/inject_items", {
      threadId,
      items: [{ type: "message", role: "assistant", content: [{ type: "output_text", text: "Fixture context." }] }],
    });
    const changed = client.notification("thread/settings/updated");
    await client.request("thread/settings/update", {
      threadId, approvalPolicy: "on-request", approvalsReviewer: "user",
      ...(scenario !== "named-extra-root" && { sandboxPolicy: scenario === "readOnly"
        ? { type: "readOnly", networkAccess: false }
        : { type: "workspaceWrite", writableRoots: [], networkAccess: false,
          excludeTmpdirEnvVar: false, excludeSlashTmp: false } }),
    });
    await changed;
    const started = await client.request("thread/resume", { threadId, cwd: workspace });
    const historyPath = started.thread.path;
    await client.close();
    const history = async () => (await readFile(historyPath, "utf8")).trim().split("\n").map(JSON.parse);
    const initialHistory = await history();
    const expected = initialHistory.filter((entry) => entry.payload?.type === "thread_settings_applied").at(-1).payload.thread_settings;
    const workspaceRoots = initialHistory.filter((entry) => entry.type === "turn_context").at(-1).payload.workspace_roots;
    // Entry order is not native precedence: specificity and access decide it.
    // Compare every rule/field while allowing native configuration to sort them.
    const canonical = (value) => {
      const result = structuredClone(value);
      result.file_system?.entries?.sort((left, right) => JSON.stringify(left).localeCompare(JSON.stringify(right)));
      return result;
    };
    const policy = (value) => ({
      approval: value.approval_policy, reviewer: value.approvals_reviewer,
      permissions: canonical(value.permission_profile),
    });
    const seenTurns = new Set(initialHistory.filter((entry) => entry.type === "turn_context").map((entry) => entry.payload.turn_id));
    for (let cycle = 0; cycle < 2; cycle++) {
      client = await rpcClient(binary, home, adapter);
      const opened = await client.request(cycle === 0 ? "session/resume" : "session/load", {
        sessionId: threadId, cwd: workspace, mcpServers: [],
      });
      assert.equal(opened.configOptions.find((option) => option.id === "mode").currentValue, "native");
      // A rejected local provider request still creates a real native turn.
      // Its result is irrelevant; exact fresh persisted policy is the evidence.
      await client.request("session/prompt", {
        sessionId: threadId, prompt: [{ type: "text", text: "Offline permission preservation check." }],
      }).catch(() => {});
      await client.close();
      const fresh = (await history()).filter((entry) => entry.type === "turn_context" && !seenTurns.has(entry.payload.turn_id));
      assert.ok(fresh.length > 0, `${scenario}: restart ${cycle + 1} must create a fresh native turn context`);
      for (const entry of fresh) {
        assert.deepEqual(policy(entry.payload), policy(expected), `${scenario}: restart ${cycle + 1} must retain the exact native policy`);
        assert.deepEqual(entry.payload.workspace_roots, workspaceRoots, `${scenario}: restart ${cycle + 1} must retain the native workspace roots`);
        seenTurns.add(entry.payload.turn_id);
      }
      console.log(`Verified actual native ${scenario}: restart ${cycle + 1}, fresh turn, exact policy.`);
    }
  } finally {
    await client?.close();
    await rm(root, { recursive: true, force: true });
  }
}

async function rpcClient(binary, root, adapter) {
  const env = { ...process.env, CODEX_HOME: root, CODEX_SQLITE_HOME: root, CODEX_PATH: path.resolve(binary),
    APP_SERVER_LOGS: path.join(root, "adapter-logs"),
    NO_PROXY: "127.0.0.1,localhost", no_proxy: "127.0.0.1,localhost" };
  delete env.MODEL_PROVIDER;
  delete env.INITIAL_AGENT_MODE;
  env.CODEX_CONFIG = JSON.stringify({ cli_auth_credentials_store: "file" });
  delete env.DEFAULT_AUTH_REQUEST;
  delete env.CODEX_ACCESS_TOKEN;
  const child = spawn(adapter ? process.execPath : path.resolve(binary),
    adapter ? [path.resolve(adapter)] : ["app-server", "--listen", "stdio://"], {
    cwd: root, env, stdio: "pipe",
    detached: process.platform !== "win32",
  });
  let sequence = 0;
  let terminal;
  let stderrBytes = 0;
  let didClose = false;
  const pending = new Map();
  const notifications = new Map();
  const lines = createInterface({ input: child.stdout });
  child.stderr.on("data", (value) => { stderrBytes += value.length; });
  const fail = (error) => {
    terminal = error;
    for (const waiter of pending.values()) waiter.reject(error);
    for (const waiter of notifications.values()) waiter.reject(error);
    pending.clear();
    notifications.clear();
  };
  child.once("error", (error) => fail(new Error(`native_spawn_failed: ${error.code}`)));
  child.stdin.on("error", (error) => fail(new Error(`native_stdin_failed: ${error.code}`)));
  const closed = new Promise((resolve) => child.once("close", (code) => {
    didClose = true;
    fail(new Error(`native_closed: code=${code}, stderr_bytes=${stderrBytes}`));
    resolve();
  }));
  lines.on("line", (line) => {
    let message;
    try { message = JSON.parse(line); } catch { fail(new Error("native_invalid_json")); return; }
    if (message.id !== undefined) {
      const waiter = pending.get(message.id);
      pending.delete(message.id);
      if (message.error) waiter?.reject(new Error(`native_rpc_error: code=${message.error.code}`));
      else waiter?.resolve(message.result);
    } else {
      const waiter = notifications.get(message.method);
      notifications.delete(message.method);
      waiter?.resolve(message.params);
    }
  });
  const wait = (map, key) => new Promise((resolve, reject) => {
    if (terminal) { reject(terminal); return; }
    const timer = setTimeout(() => {
      map.delete(key);
      reject(new Error(`native_timeout: key=${key}, stderr_bytes=${stderrBytes}`));
    }, 15000);
    map.set(key, {
      resolve: (value) => { clearTimeout(timer); resolve(value); },
      reject: (error) => { clearTimeout(timer); reject(error); },
    });
  });
  const request = (method, params) => {
    const id = ++sequence;
    const result = wait(pending, id).catch((error) => {
      throw new Error(`native_request_failed: method=${method}; ${error.message}`);
    });
    child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
    return result;
  };
  const close = async () => {
    if (!didClose && !child.stdin.destroyed) child.stdin.end();
    const timer = setTimeout(killOwnedProcesses, 3000);
    try { await closed; } finally {
      clearTimeout(timer);
      // A closed adapter can leave its native child alive. The fixture owns
      // the entire process group even when its original leader has exited.
      killOwnedProcesses();
      lines.close();
    }
  };
  function killOwnedProcesses() {
    if (!child.pid) return;
    try {
      if (process.platform === "win32") child.kill("SIGKILL");
      else process.kill(-child.pid, "SIGKILL");
    } catch (error) { if (error.code !== "ESRCH") throw error; }
  }
  try {
    await request("initialize", adapter ? { protocolVersion: 1, clientCapabilities: {},
      clientInfo: { name: "openaide-recovery-fixture", version: "1" } } : {
      clientInfo: { name: "openaide-recovery-fixture", version: "1" },
      capabilities: { experimentalApi: true },
    });
    if (!adapter) child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method: "initialized", params: {} })}\n`);
  } catch (error) { await close(); throw error; }
  return { request, notification: (method) => wait(notifications, method), close };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (!process.argv[2] || !process.argv[3]) throw new Error("Usage: node scripts/smoke-codex-native-recovery.mjs <native-codex-binary> <adapter-entrypoint>");
  await smokeCodexNativeRecovery(process.argv[2], process.argv[3]);
}
