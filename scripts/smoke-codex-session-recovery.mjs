import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import path from "node:path";
import os from "node:os";

const native = fileURLToPath(new URL("./fixtures/codex-session-recovery-native.mjs", import.meta.url));
const askPolicy = {
  approvalPolicy: "on-request", approvalsReviewer: "user",
  sandbox: { type: "workspaceWrite", writableRoots: [], networkAccess: false,
    excludeTmpdirEnvVar: false, excludeSlashTmp: false },
};

const cases = [
  { name: "preset", policy: askPolicy, mode: "native" },
  { name: "explicit-provider", policy: askPolicy, mode: "native", provider: "fixture-provider" },
  { name: "read-only-sandbox", policy: { ...askPolicy, sandbox: { type: "readOnly", networkAccess: false } }, mode: "native" },
  { name: "external-sandbox", policy: { ...askPolicy, sandbox: { type: "externalSandbox", networkAccess: "enabled" } }, rejected: true },
  { name: "custom-workspace", policy: { ...askPolicy, sandbox: {
    ...askPolicy.sandbox, writableRoots: ["/fixture-extra"], networkAccess: true,
    excludeTmpdirEnvVar: true, excludeSlashTmp: true,
  } }, mode: "native" },
  { name: "granular-approval", policy: { ...askPolicy, approvalPolicy: { granular: {
    sandbox_approval: true, rules: false, skill_approval: true,
    request_permissions: false, mcp_elicitations: true,
  } } }, mode: "native", changePreset: true },
  { name: "persisted-sandbox", policy: askPolicy,
    globalSandbox: { type: "dangerFullAccess" }, historyFixture: true },
];

/** Exercise the installed adapter against a native wire fixture without model calls. */
export async function smokeCodexSessionRecovery(adapter) {
  for (const method of ["session/resume", "session/load"]) {
    for (const scenario of cases) await verifyRecovery(adapter, method, scenario);
  }
}

async function verifyRecovery(adapter, method, scenario) {
  const { policy, mode, provider } = scenario;
  const root = await mkdtemp(path.join(os.tmpdir(), "openaide-codex-recovery-"));
  const callsPath = path.join(root, "calls.jsonl");
  const fixturePath = path.join(root, "fixture.json");
  const historyPath = path.join(root, "history.jsonl");
  let history;
  if (scenario.historyFixture) {
    const source = await readFile(new URL("./fixtures/codex-native-recovery-history.json", import.meta.url), "utf8");
    history = JSON.parse(source.replaceAll("$FIXTURE_ROOT", root).replaceAll("$THREAD_ID", "native-session"));
  } else {
    const sandbox = policy.sandbox;
    const nativeTypes = { workspaceWrite: "workspace-write", readOnly: "read-only", externalSandbox: "external-sandbox" };
    const fields = { networkAccess: "network_access", writableRoots: "writable_roots",
      excludeTmpdirEnvVar: "exclude_tmpdir_env_var", excludeSlashTmp: "exclude_slash_tmp" };
    history = [{ type: "turn_context", payload: {
      cwd: root, approval_policy: policy.approvalPolicy, approvals_reviewer: policy.approvalsReviewer,
      sandbox_policy: { type: nativeTypes[sandbox.type], ...Object.fromEntries(Object.entries(sandbox)
        .filter(([key]) => fields[key]).map(([key, value]) => [fields[key], value])) },
    } }];
  }
  history.unshift({ type: "session_meta", payload: { id: "native-session" } });
  await writeFile(historyPath, `${history.map((entry) => JSON.stringify(entry)).join("\n")}\n`);
  await writeFile(fixturePath, JSON.stringify({
    cwd: root, calls: callsPath, policy, historyPath, globalSandbox: scenario.globalSandbox,
  }));
  const env = { ...process.env, CODEX_PATH: native, CODEX_HOME: root, CODEX_SQLITE_HOME: root,
    APP_SERVER_LOGS: root, OPENAIDE_CODEX_RECOVERY_FIXTURE: fixturePath };
  delete env.MODEL_PROVIDER;
  delete env.INITIAL_AGENT_MODE;
  env.CODEX_CONFIG = JSON.stringify({ cli_auth_credentials_store: "file" });
  delete env.DEFAULT_AUTH_REQUEST;
  delete env.CODEX_ACCESS_TOKEN;
  if (provider) env.MODEL_PROVIDER = provider;
  const child = spawn(process.execPath, [path.resolve(adapter)], {
    env,
    stdio: "pipe",
    detached: process.platform !== "win32",
  });
  let stderrBytes = 0;
  child.stderr.on("data", (chunk) => { stderrBytes += chunk.length; });
  const pending = new Map();
  let terminalError;
  const rejectPending = (error) => {
    terminalError = error;
    for (const waiter of pending.values()) waiter.reject(error);
    pending.clear();
  };
  child.once("error", (error) => rejectPending(new Error(`adapter_spawn_failed: ${error.code ?? "unknown"}`)));
  child.stdin.on("error", (error) => rejectPending(new Error(`adapter_stdin_failed: ${error.code ?? "unknown"}`)));
  const closed = new Promise((resolve) => child.once("close", (code, signal) => {
    rejectPending(new Error(`adapter_closed: code=${code}, signal=${signal}, stderr_bytes=${stderrBytes}`));
    resolve();
  }));
  const lines = createInterface({ input: child.stdout });
  lines.on("line", (line) => {
    let message;
    try { message = JSON.parse(line); } catch {
      rejectPending(new Error("adapter_invalid_json"));
      return;
    }
    if (message.id === undefined) return;
    const waiter = pending.get(String(message.id));
    if (!waiter) return;
    pending.delete(String(message.id));
    if (message.error) waiter.reject(new Error(`${waiter.method}: rpc_error=${message.error.code}`));
    else waiter.resolve(message.result);
  });
  let id = 0;
  const request = (method, params) => new Promise((resolve, reject) => {
    if (terminalError) { reject(terminalError); return; }
    const requestId = String(++id);
    const timeout = setTimeout(() => {
      pending.delete(requestId);
      reject(new Error(`${method}: timed_out, stderr_bytes=${stderrBytes}`));
    }, 10000);
    pending.set(String(requestId), {
      method,
      resolve: (result) => { clearTimeout(timeout); resolve(result); },
      reject: (error) => { clearTimeout(timeout); reject(error); },
    });
    child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id: requestId, method, params })}\n`);
  });
  try {
    await request("initialize", { protocolVersion: 1, clientCapabilities: {}, clientInfo: { name: "recovery-fixture", version: "1" } });
    if (scenario.rejected) {
      await assert.rejects(request(method, { sessionId: "native-session", cwd: root, mcpServers: [] }));
      assert.ok(!(await readCalls()).some((call) => call.method === "thread/resume"), "unsupported history must fail before native resume");
      console.log(`Verified ${method}: ${scenario.name} rejected before resume.`);
      return;
    }
    const resumed = await request(method, { sessionId: "native-session", cwd: root, mcpServers: [] });
    const expectedModel = provider ? "native-astra" : "native-luna";
    assert.equal(resumed.configOptions.find((option) => option.id === "model").currentValue, expectedModel);
    const modeOption = resumed.configOptions.find((option) => option.id === "mode");
    if (mode) assert.equal(modeOption.currentValue, mode);
    const selectedMode = modeOption.currentValue;
    assert.ok(modeOption.options.some((option) => option.value === selectedMode));
    assert.equal(resumed.modes.currentModeId, selectedMode);
    assert.ok(resumed.modes.availableModes.some((option) => option.id === selectedMode));
    if (selectedMode === "native") {
      const reselected = await request("session/set_config_option", {
        sessionId: "native-session", configId: "mode", value: "native",
      });
      assert.equal(reselected.configOptions.find((option) => option.id === "mode").currentValue, "native");
    }
    await request("session/prompt", { sessionId: "native-session", prompt: [{ type: "text", text: "Fixture only" }] });
    let calls = await readCalls();
    assert.equal(calls.find((call) => call.method === "thread/resume").params.modelProvider, provider);
    const sent = calls.find((call) => call.method === "turn/start").params;
    assert.equal(sent.model, expectedModel);
    assert.deepEqual(sent.approvalPolicy, policy.approvalPolicy);
    assert.equal(sent.approvalsReviewer, policy.approvalsReviewer);
    assert.equal(sent.sandboxPolicy, undefined, "unchanged turns must retain the full native permissions");
    assert.equal(sent.permissions, undefined);
    assert.deepEqual(calls.find((call) => call.method === "fixture/turnPolicy").params.sandboxPolicy, policy.sandbox);
    if (calls.some((call) => call.method === "thread/settings/update")) {
      assert.ok(calls.findIndex((call) => call.method === "fixture/settingsCommitted")
        < calls.findIndex((call) => call.method === "turn/start"), "resume must await the terminal settings update");
    }
    if (scenario.changePreset) {
      const changed = await request("session/set_config_option", {
        sessionId: "native-session", configId: "mode", value: "agent",
      });
      assert.equal(changed.configOptions.find((option) => option.id === "mode").currentValue, "agent");
      await request("session/prompt", { sessionId: "native-session", prompt: [{ type: "text", text: "Explicit preset" }] });
      calls = await readCalls();
      const changedTurn = calls.filter((call) => call.method === "turn/start").at(-1).params;
      assert.equal(changedTurn.approvalPolicy, "on-request");
      assert.equal(changedTurn.approvalsReviewer, "auto_review");
      assert.deepEqual(changedTurn.sandboxPolicy, askPolicy.sandbox);
    }
    console.log(`Verified ${method}: ${scenario.name}.`);
  } catch (error) {
    const methods = (await readCalls().catch(() => [])).map((call) => call.method).join(",");
    throw new Error(`${scenario.name}: ${error.message}; methods=${methods}`, { cause: error });
  } finally {
    rejectPending(new Error("fixture_closed"));
    if (!child.stdin.destroyed) child.stdin.end();
    // This fixture owns the adapter and its native subprocess together. Escalate
    // the whole group if a failed adapter cannot perform its normal shutdown.
    const kill = setTimeout(killOwnedProcesses, 3000);
    try { await closed; } finally {
      clearTimeout(kill);
      killOwnedProcesses();
      lines.close();
      await rm(root, { recursive: true, force: true });
    }
  }

  async function readCalls() {
    return (await readFile(callsPath, "utf8")).trim().split("\n").map(JSON.parse);
  }

  function killOwnedProcesses() {
    if (!child.pid) return;
    try {
      if (process.platform === "win32") child.kill("SIGKILL");
      else process.kill(-child.pid, "SIGKILL");
    } catch (error) {
      if (error.code !== "ESRCH") throw error;
    }
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [adapter] = process.argv.slice(2);
  if (!adapter) throw new Error("Usage: node scripts/smoke-codex-session-recovery.mjs <adapter-entrypoint>");
  await smokeCodexSessionRecovery(adapter);
}
