import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import path from "node:path";
import os from "node:os";

const native = fileURLToPath(new URL("./fixtures/codex-native-subagents.mjs", import.meta.url));

/** Uses the installed adapter bytes and an isolated native fixture, with no model or caller credentials. */
export async function smokeCodexSubagents(adapter) {
  for (const enabled of [true, false]) await checkSubagents(adapter, enabled);
}

async function checkSubagents(adapter, enabled) {
  const root = await mkdtemp(path.join(os.tmpdir(), "openaide-codex-subagents-"));
  const fixture = path.join(root, "fixture.json");
  const history = path.join(root, "history.jsonl");
  await writeFile(history, [
    { type: "session_meta", payload: { id: "native-session" } },
    { type: "turn_context", payload: { cwd: root, approval_policy: "never", approvals_reviewer: "user", sandbox_policy: { type: "danger-full-access" } } },
  ].map((entry) => JSON.stringify(entry)).join("\n") + "\n");
  await writeFile(fixture, JSON.stringify({ cwd: root, calls: path.join(root, "calls.jsonl"), historyPath: history }));
  const env = {
    ...process.env, CODEX_PATH: native, CODEX_HOME: root, CODEX_SQLITE_HOME: root,
    CODEX_CONFIG: JSON.stringify({ cli_auth_credentials_store: "file" }),
    APP_SERVER_LOGS: root, OPENAIDE_CODEX_SUBAGENTS_FIXTURE: fixture,
  };
  for (const name of ["MODEL_PROVIDER", "INITIAL_AGENT_MODE", "DEFAULT_AUTH_REQUEST", "CODEX_API_KEY", "OPENAI_API_KEY", "CODEX_ACCESS_TOKEN"]) delete env[name];
  const child = spawn(process.execPath, [path.resolve(adapter)], {
    env, stdio: "pipe", detached: process.platform !== "win32",
  });
  const pending = new Map();
  const updates = [];
  let nextId = 0;
  let stderrBytes = 0;
  let terminalError;
  child.stderr.on("data", (chunk) => { stderrBytes += chunk.length; });
  const fail = (error) => {
    terminalError = error;
    for (const waiter of pending.values()) waiter.reject(error);
    pending.clear();
  };
  const closed = new Promise((resolve) => child.once("close", resolve));
  child.once("error", () => fail(new Error("adapter_spawn_failed")));
  child.once("close", (code) => fail(new Error(`adapter_closed: code=${code}; stderr_bytes=${stderrBytes}`)));
  child.stdin.on("error", () => fail(new Error("adapter_stdin_failed")));
  const lines = createInterface({ input: child.stdout });
  lines.on("line", (line) => {
    let message;
    try { message = JSON.parse(line); } catch { fail(new Error("adapter_invalid_json")); return; }
    if (message.method === "session/update") updates.push(message.params);
    if (message.id === undefined || message.method) return;
    const waiter = pending.get(String(message.id));
    pending.delete(String(message.id));
    if (message.error) waiter?.reject(new Error(`adapter_rpc_error: code=${message.error.code}`));
    else waiter?.resolve(message.result);
  });
  async function request(method, params) {
    if (terminalError) throw terminalError;
    const id = String(++nextId);
    let timer;
    try {
      return await new Promise((resolve, reject) => {
        timer = setTimeout(() => {
          pending.delete(id);
          reject(new Error(`adapter_timeout: method=${method}; stderr_bytes=${stderrBytes}`));
        }, 15_000);
        pending.set(id, { resolve, reject });
        child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
      });
    } finally { clearTimeout(timer); }
  }
  function assertNativeHistory(events) {
    const childId = "native-session:subagent:child-fixture";
    const lifecycle = events.filter(({ sessionId, update }) => sessionId === "native-session"
      && update.sessionUpdate === "subagent_update" && update.sessionId === childId);
    const spawn = lifecycle[0];
    assert.equal(spawn?.update.description, "Review the fixture", "child must be announced on its parent");
    const childEvents = events.filter(({ sessionId }) => sessionId === childId);
    assert.ok(childEvents.some(({ update }) => update.sessionUpdate === "agent_message_chunk"
      && update.content?.text === "Packaged child result"), "child text must have its own session");
    const prompts = events.filter(({ update }) => update.sessionUpdate === "session_message");
    assert.deepEqual(prompts.map(({ sessionId }) => sessionId), ["native-session", childId],
      "the delegated prompt must reach the parent and open the child");
    assert.ok(prompts[1].update.senderMessageId, "the child's view must link to the parent's");
    assert.deepEqual(prompts[1].update.content, [{ type: "text", text: "Review the fixture" }]);
    assert.ok(events.every(({ update }) => !update.messageId?.endsWith(":subagent-history-gap")),
      "complete child history must not report a gap");
    const terminal = lifecycle.filter(({ update }) => update.state?.state === "idle");
    assert.equal(terminal.length, 1, "announced child must terminate exactly once");
    assert.equal(terminal[0].update.state.stopReason, "end_turn");
    assert.ok(events.indexOf(spawn) < events.indexOf(childEvents[0]), "announcement must precede child traffic");
  }
  try {
    const clientCapabilities = enabled ? {
      subagents: {}, _meta: { openaide: { nativeSubagentSessions: true } },
    } : {};
    const initialized = await request("initialize", { protocolVersion: 1, clientCapabilities });
    assert.deepEqual(initialized.agentCapabilities.sessionCapabilities.subagents, {});
    await request("session/load", { sessionId: "native-session", cwd: root, mcpServers: [] });
    updates.length = 0;
    await request("session/prompt", { sessionId: "native-session", prompt: [{ type: "text", text: "Subagent fixture" }] });
    if (enabled) {
      assertNativeHistory(updates);
      updates.length = 0;
      await request("session/load", { sessionId: "native-session", cwd: root, mcpServers: [] });
      assertNativeHistory(updates);
      console.log("Verified packaged Codex ACP native child routing, terminal state, and authoritative replay.");
    } else {
      assert.ok(updates.every(({ update }) => !update.sessionUpdate.startsWith("subagent_")));
      const wait = updates.find(({ update }) => update.sessionUpdate === "tool_call" && update.title === "wait");
      assert.ok(wait?.update.rawInput?.senderThreadId);
      assert.ok(Array.isArray(wait.update.rawInput.receiverThreadIds));
      console.log("Verified packaged Codex ACP legacy collaboration when native support is disabled.");
    }
  } finally {
    fail(new Error("fixture_closed"));
    child.stdin.end();
    const killOwned = () => {
      if (!child.pid) return;
      try {
        if (process.platform === "win32") child.kill("SIGKILL");
        else process.kill(-child.pid, "SIGKILL");
      } catch (error) { if (error.code !== "ESRCH") throw error; }
    };
    const timer = setTimeout(killOwned, 3_000);
    try { await closed; } finally {
      clearTimeout(timer);
      killOwned();
      lines.close();
      await rm(root, { recursive: true, force: true });
    }
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  if (!process.argv[2]) throw new Error("Usage: node scripts/smoke-codex-subagents.mjs <adapter-entrypoint>");
  await smokeCodexSubagents(process.argv[2]);
}
