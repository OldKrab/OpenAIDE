import assert from "node:assert/strict";
import { chmod, mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import * as recovery from "./session-recovery.mjs";

const restricted = {
  type: "managed", network: "restricted",
  file_system: { type: "restricted", entries: [
    { path: { type: "special", value: { kind: "root" } }, access: "read" },
  ] },
};
const settings = (profile = restricted) => ({
  type: "event_msg", payload: { type: "thread_settings_applied", thread_id: "session",
    thread_settings: { approval_policy: "on-request", approvals_reviewer: "user", permission_profile: profile },
  },
});

async function withHistory(records, run) {
  const root = await mkdtemp(path.join(os.tmpdir(), "openaide-history-recovery-test-"));
  const history = path.join(root, "history.jsonl");
  try {
    await writeFile(history, [
      JSON.stringify({ type: "session_meta", payload: { id: "session" } }),
      ...records.map((record) => typeof record === "string" ? record : JSON.stringify(record)),
    ].join("\n") + "\n");
    const calls = [];
    const observers = new Set();
    const closed = new Set();
    const subscribe = (set) => (listener) => { set.add(listener); return { dispose: () => set.delete(listener) }; };
    const client = {
      connection: { onUnhandledNotification: subscribe(observers), onClose: subscribe(closed), onDispose: subscribe(closed) },
      emitSettings(params) { for (const observer of observers) observer({ method: "thread/settings/updated", params }); },
      observerCount: () => observers.size + closed.size,
      async threadRead(params) {
        calls.push({ method: "read", params });
        return { thread: { id: "session", path: history } };
      },
      async threadResume(params) {
        calls.push({ method: "resume", params: JSON.parse(JSON.stringify(params)) });
        return { thread: { id: "session" }, model: "native-model", sandbox: { type: "dangerFullAccess" } };
      },
      async threadSettingsUpdate(params) {
        calls.push({ method: "update", params });
        queueMicrotask(() => client.emitSettings({ threadId: params.threadId, threadSettings: params }));
      },
      async threadUnsubscribe(params) { calls.push({ method: "unsubscribe", params }); return { status: "unsubscribed" }; },
    };
    await run(client, calls, history);
  } finally { await rm(root, { recursive: true, force: true }); }
}

test("restores the last native settings before resume and preserves explicit provider", async () => {
  await withHistory([settings({ type: "disabled" }), settings()], async (client, calls) => {
    const result = await recovery.resumeNativeSession(client, { threadId: "session", modelProvider: "explicit" });
    assert.deepEqual(calls.map((call) => call.method), ["read", "resume"]);
    assert.deepEqual(calls[0].params, { threadId: "session", includeTurns: false });
    const resumed = calls[1].params;
    assert.equal(resumed.modelProvider, "explicit");
    assert.equal(resumed.approvalPolicy, "on-request");
    assert.equal(resumed.approvalsReviewer, "user");
    assert.equal(resumed.sandbox, undefined);
    assert.deepEqual(resumed.config.permissions[resumed.permissions], {
      filesystem: { ":root": "read" }, network: { enabled: false },
    });
    assert.equal(result.usesNativePermissions, true);
    assert.equal(result.model, "native-model");
    assert.equal(result.approvalPolicy, "on-request");
  });
});

test("a rejected settings update releases the native subscription before reporting recovery failure", async () => {
  await withHistory([workspaceSettings()], async (client, calls) => {
    client.threadSettingsUpdate = async () => { throw Object.assign(new Error("fixture RPC rejection"), { code: -32602 }); };
    const events = [];
    await assert.rejects(recovery.resumeNativeSession(client, { threadId: "session" }, { log: (event) => events.push(event) }), /Reopen.*retry/);
    assert.deepEqual(calls.map((call) => call.method), ["read", "resume", "unsubscribe"]);
    assert.equal(client.observerCount(), 0);
    assert.deepEqual(events.map((event) => event.phase), ["start", "terminal"]);
    assert.equal(events[1].outcome, "failure");
    assert.equal(events[1].error_code, "settings_request_failed");
    assert.equal(events[1].attempt, 1);
    assert.equal(typeof events[1].duration_ms, "number");
  });
});

test("materialized permissions keep native workspace scope across a settings-only update", async () => {
  const cwd = path.resolve(os.tmpdir(), "native-cwd");
  const roots = [cwd, path.resolve(os.tmpdir(), "native-extra")];
  const newer = settings();
  newer.payload.thread_settings.cwd = cwd;
  await withHistory([
    { type: "turn_context", payload: { cwd, workspace_roots: roots, approval_policy: "on-request",
      approvals_reviewer: "user", permission_profile: restricted } }, newer,
  ], async (client, calls) => {
    await recovery.resumeNativeSession(client, { threadId: "session", cwd });
    assert.deepEqual(calls[1].params.runtimeWorkspaceRoots, roots);
  });
});

function workspaceSettings(cwd = path.resolve(os.tmpdir(), "native-project")) {
  const value = settings({ type: "managed", network: "restricted", file_system: {
    type: "restricted", entries: [
      ...restricted.file_system.entries,
      { path: { type: "path", path: cwd }, access: "write" },
      ...["tmpdir", "slash_tmp"].map((kind) => ({ path: { type: "special", value: { kind } }, access: "write" })),
      ...[".git", ".agents", ".codex", ".codex"].map((name) => ({
        path: { type: "path", path: path.join(cwd, name) }, access: "read", missing_path_behavior: "skip",
      })),
    ],
  } });
  value.payload.thread_settings.cwd = cwd;
  return value;
}

test("native workspace reconstruction preserves protected skip rules and duplicate entries", async () => {
  const saved = workspaceSettings();
  await withHistory([saved], async (client, calls) => {
    const cwd = saved.payload.thread_settings.cwd;
    await recovery.resumeNativeSession(client, { threadId: "session", cwd });
    assert.equal(calls[1].params.sandbox, "read-only");
    assert.deepEqual(calls[1].params.config.sandbox_workspace_write, {
      writable_roots: [], network_access: false,
      exclude_tmpdir_env_var: false, exclude_slash_tmp: false,
    });
    assert.equal(calls[1].params.permissions, undefined);
    assert.deepEqual(calls[2].params.sandboxPolicy, {
      type: "workspaceWrite", writableRoots: [], networkAccess: false,
      excludeTmpdirEnvVar: false, excludeSlashTmp: false,
    });
    assert.equal(client.observerCount(), 0);
  });
});

test("legacy read-only histories retain user review when automatic reviewers did not exist", async () => {
  await withHistory([{ type: "turn_context", payload: {
    approval_policy: "on-request", sandbox_policy: { type: "read-only" },
  } }], async (client, calls) => {
    await recovery.resumeNativeSession(client, { threadId: "session" });
    const resumed = calls[1].params;
    assert.equal(resumed.approvalsReviewer, "user");
    assert.deepEqual(resumed.config.permissions[resumed.permissions], {
      filesystem: { ":root": "read" }, network: { enabled: false },
    });
  });
});

test("prepared sessions without policy use the adapter initial mode", async () => {
  await withHistory([], async (client, calls) => {
    const result = await recovery.resumeNativeSession(client, { threadId: "session" });
    assert.deepEqual(calls[1].params, { threadId: "session" });
    const initial = {};
    assert.equal(recovery.recoveredSessionMode(result, { getInitialAgentMode: () => initial }), initial);
  });
});

const unsupportedHistories = [
  ["corrupt trailing record", [settings(), "{\"type\":"]],
  ["different session identity", [{ type: "session_meta", payload: { id: "another" } }, settings()]],
  ["latest unknown profile", [settings(), settings({ type: "future" })]],
  ["unrestricted managed filesystem", [settings({ type: "managed", network: "restricted", file_system: { type: "unrestricted" } })]],
  ["external sandbox", [settings({ type: "external", network: "enabled" })]],
  ["literal path interpreted as glob", [settings({ ...restricted, file_system: { type: "restricted", entries: [
    { path: { type: "path", path: path.resolve(os.tmpdir(), "project[1]") }, access: "read" },
  ] } })]],
  ["unknown full policy field", [settings({ ...restricted, future_rule: false })]],
  ["missing policy after accepted history", [{ type: "response_item", payload: { type: "message", role: "user", content: [] } }]],
  ["network-enabled generated profile", [settings({ ...restricted, network: "enabled" })]],
  ["custom protected carveout", [(() => {
    const value = workspaceSettings();
    value.payload.thread_settings.permission_profile.file_system.entries.push({
      path: { type: "path", path: path.resolve(os.tmpdir(), "outside-worktree-admin") },
      access: "read", missing_path_behavior: "skip",
    });
    return value;
  })()]],
];
for (const [name, records] of unsupportedHistories) {
  test(`rejects ${name} before native resume can alter history`, async () => {
    await withHistory(records, async (client, calls) => {
      await assert.rejects(recovery.resumeNativeSession(client, { threadId: "session" }), /Cannot safely restore/);
      assert.deepEqual(calls.map((call) => call.method), ["read"]);
    });
  });
}

test("native rule conflict precedence and scoped deny globs survive JSON RPC", async () => {
  const value = settings({ ...restricted, file_system: { type: "restricted", glob_scan_max_depth: 3, entries: [
    ...restricted.file_system.entries,
    { path: { type: "special", value: { kind: "project_roots" } }, access: "write" },
    { path: { type: "glob_pattern", pattern: "codex-project-roots://**/*.env" }, access: "deny" },
    { path: { type: "special", value: { kind: "project_roots" } }, access: "deny" },
    { path: { type: "special", value: { kind: "project_roots" } }, access: "read" },
  ] } });
  value.payload.thread_settings.workspace_roots = [path.resolve(os.tmpdir(), "historical-root")];
  await withHistory([value], async (client, calls) => {
    await recovery.resumeNativeSession(client, { threadId: "session" });
    const resumed = calls[1].params;
    assert.deepEqual(resumed.runtimeWorkspaceRoots, value.payload.thread_settings.workspace_roots);
    assert.deepEqual(resumed.config.permissions[resumed.permissions].filesystem, {
      glob_scan_max_depth: 3, ":root": "read", ":workspace_roots": { ".": "deny", "**/*.env": "deny" },
    });
  });
});

test("worktree admin protection must match the current Git backlink", async () => {
  await withHistory([], async (client, calls, history) => {
    const root = path.dirname(history);
    const cwd = path.join(root, "workspace");
    const gitdir = path.join(root, "repository", ".git", "worktrees", "workspace");
    await mkdir(cwd);
    await mkdir(gitdir, { recursive: true });
    await writeFile(path.join(cwd, ".git"), `gitdir: ${gitdir}\n`);
    await writeFile(path.join(gitdir, "commondir"), "../..\n");
    await writeFile(path.join(gitdir, "gitdir"), `${path.join(cwd, ".git")}\n`);
    const saved = workspaceSettings(cwd);
    saved.payload.thread_settings.permission_profile.file_system.entries.push({
      path: { type: "path", path: gitdir }, access: "read", missing_path_behavior: "skip",
    });
    await writeFile(history, `${JSON.stringify({ type: "session_meta", payload: { id: "session" } })}\n${JSON.stringify(saved)}\n`);
    await recovery.resumeNativeSession(client, { threadId: "session", cwd });
    assert.deepEqual(calls.map((call) => call.method), ["read", "resume", "update"]);
    calls.length = 0;
    await writeFile(path.join(gitdir, "gitdir"), `${path.join(root, "unrelated")}\n`);
    await assert.rejects(recovery.resumeNativeSession(client, { threadId: "session", cwd }), /Cannot safely restore/);
    assert.deepEqual(calls.map((call) => call.method), ["read"]);
  });
});

test("a readable stat followed by stream access denial fails without native mutation", { skip: process.platform === "win32" }, async () => {
  await withHistory([settings()], async (client, calls, history) => {
    await chmod(history, 0);
    await assert.rejects(recovery.resumeNativeSession(client, { threadId: "session" }), /Cannot safely restore/);
    assert.deepEqual(calls.map((call) => call.method), ["read"]);
  });
});

test("invalid UTF-8 cannot silently change a stored permission path", async () => {
  await withHistory([settings()], async (client, calls, history) => {
    const prefix = `${JSON.stringify({ type: "session_meta", payload: { id: "session" } })}\n`;
    const value = settings({ ...restricted, file_system: { type: "restricted", entries: [
      { path: { type: "path", path: path.resolve(os.tmpdir(), "native-INVALID") }, access: "read" },
    ] } });
    const [before, after] = JSON.stringify(value).split("INVALID");
    await writeFile(history, Buffer.concat([Buffer.from(prefix + before), Buffer.from([255]), Buffer.from(after + "\n")]));
    await assert.rejects(recovery.resumeNativeSession(client, { threadId: "session" }), /Cannot safely restore/);
    assert.deepEqual(calls.map((call) => call.method), ["read"]);
  });
});

test("workspace recovery waits for terminal settings rather than the update ACK", async () => {
  await withHistory([workspaceSettings()], async (client) => {
    let updateStarted;
    const update = new Promise((resolve) => { updateStarted = resolve; });
    client.threadSettingsUpdate = async (params) => { updateStarted(params); };
    let settled = false;
    const pending = recovery.resumeNativeSession(client, { threadId: "session" }).then((value) => { settled = true; return value; });
    const params = await update;
    await new Promise(setImmediate);
    assert.equal(settled, false);
    client.emitSettings({ threadId: params.threadId, threadSettings: params });
    assert.equal((await pending).usesNativePermissions, true);
    assert.equal(client.observerCount(), 0);
  });
});

test("ambiguous settings recovery releases its subscription and fences late replies until reconnection", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  await withHistory([workspaceSettings()], async (client, calls) => {
    let updateStarted;
    let finishLateRequest;
    const update = new Promise((resolve) => { updateStarted = resolve; });
    client.threadSettingsUpdate = (params) => { updateStarted(params); return new Promise((resolve) => { finishLateRequest = resolve; }); };
    let settled = false;
    const pending = recovery.resumeNativeSession(client, { threadId: "session" });
    pending.then(() => { settled = true; }, () => { settled = true; });
    const params = await update;
    client.emitSettings({ threadId: params.threadId, threadSettings: params });
    t.mock.timers.tick(15001);
    await new Promise(setImmediate);
    assert.equal(settled, true);
    await assert.rejects(pending, /Restart the OpenAIDE App Server/);
    assert.equal(client.observerCount(), 0);
    assert.equal(calls.at(-1).method, "unsubscribe");
    const beforeRetry = calls.length;
    finishLateRequest();
    await assert.rejects(recovery.resumeNativeSession(client, { threadId: "session" }), /Restart the OpenAIDE App Server/);
    assert.equal(calls.length, beforeRetry);
  });
  await withHistory([workspaceSettings()], async (freshClient) => {
    assert.equal((await recovery.resumeNativeSession(freshClient, { threadId: "session" })).usesNativePermissions, true);
  });
});

test("unsubscribe failure is bounded and prevents a late unsubscribe from crossing retry", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  await withHistory([workspaceSettings()], async (client, calls) => {
    let unsubscribeStarted;
    const cleanup = new Promise((resolve) => { unsubscribeStarted = resolve; });
    client.threadSettingsUpdate = async () => { throw Object.assign(new Error("fixture RPC rejection"), { code: -32602 }); };
    client.threadUnsubscribe = () => { unsubscribeStarted(); return new Promise(() => {}); };
    const pending = recovery.resumeNativeSession(client, { threadId: "session" });
    const rejected = assert.rejects(pending, /Restart the OpenAIDE App Server/);
    await cleanup;
    t.mock.timers.tick(5001);
    await rejected;
    assert.equal(client.observerCount(), 0);
    const beforeRetry = calls.length;
    await assert.rejects(recovery.resumeNativeSession(client, { threadId: "session" }), /Restart the OpenAIDE App Server/);
    assert.equal(calls.length, beforeRetry);
  });
});

for (const code of [-32099, undefined]) {
  test(`local or unknown request failure ${code ?? "without code"} cannot reuse an uncertain native session`, async () => {
    await withHistory([workspaceSettings()], async (client, calls) => {
      client.threadSettingsUpdate = async () => { throw Object.assign(new Error("fixture transport failure"), { code }); };
      await assert.rejects(recovery.resumeNativeSession(client, { threadId: "session" }), /Restart the OpenAIDE App Server/);
      assert.equal(calls.at(-1).method, "unsubscribe");
      assert.equal(client.observerCount(), 0);
      const beforeRetry = calls.length;
      await assert.rejects(recovery.resumeNativeSession(client, { threadId: "session" }), /Restart the OpenAIDE App Server/);
      assert.equal(calls.length, beforeRetry);
    });
  });
}
