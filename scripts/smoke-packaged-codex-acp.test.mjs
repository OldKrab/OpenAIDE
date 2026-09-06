import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { chmod, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const smoke = fileURLToPath(new URL("./smoke-packaged-codex-acp.mjs", import.meta.url));

// Exercise the CLI's actual App Server -> native child environment boundary.
// The fixture models preparation failing through the adapter's logout handler;
// every credential here is synthetic and both possible homes are test-owned.
for (const explicitCodexHome of [false, true]) {
  test(`packaged preparation failure preserves caller auth (${explicitCodexHome ? "CODEX_HOME" : "HOME fallback"})`,
    { skip: process.platform === "win32", timeout: 30_000 }, async (t) => {
      const root = await mkdtemp(path.join(os.tmpdir(), "openaide-package-auth-test-"));
      try {
        const callerHome = path.join(root, "caller");
        const callerCodex = path.join(callerHome, explicitCodexHome ? "custom-codex" : ".codex");
        const workspace = path.join(root, "project");
        const report = path.join(root, "report.json");
        await mkdir(callerCodex, { recursive: true });
        await mkdir(workspace);
        await writeFile(path.join(callerCodex, "auth.json"), "synthetic caller credential");
        const fixture = path.join(root, "app-server.mjs");
        await writeFile(fixture, fixtureSource);
        await chmod(fixture, 0o700);
        const env = {
          ...process.env, HOME: callerHome, USERPROFILE: callerHome,
          CODEX_HOME: callerCodex, CODEX_SQLITE_HOME: path.join(callerHome, "sqlite"),
          APP_SERVER_LOGS: path.join(callerHome, "adapter-logs"),
          CODEX_CONFIG: JSON.stringify({ log_dir: path.join(callerHome, "native-logs") }),
          CODEX_PATH: path.join(callerHome, "caller-native"),
          DEFAULT_AUTH_REQUEST: "synthetic-caller-auth", MODEL_PROVIDER: "synthetic-provider",
          INITIAL_AGENT_MODE: "synthetic-mode", CODEX_API_KEY: "synthetic-key", OPENAI_API_KEY: "synthetic-key",
          CODEX_ACCESS_TOKEN: "synthetic-access-token",
          OPENAIDE_PACKAGE_FIXTURE_ROOT: root, OPENAIDE_PACKAGE_FIXTURE_REPORT: report,
        };
        if (!explicitCodexHome) delete env.CODEX_HOME;
        const child = spawn(process.execPath, [smoke, fixture, workspace], {
          cwd: callerHome, env, signal: t.signal, detached: true, stdio: ["ignore", "pipe", "pipe"],
        });
        t.after(() => {
          if (!child.pid) return;
          try { process.kill(-child.pid, "SIGKILL"); }
          catch (error) { if (error.code !== "ESRCH") throw error; }
        });
        let stderr = "";
        child.stdout.resume();
        child.stderr.setEncoding("utf8").on("data", (chunk) => { stderr += chunk; });
        const code = await new Promise((resolve, reject) => {
          child.once("error", reject);
          child.once("close", resolve);
        });
        assert.equal(code, 1, "fixture must exercise preparation failure, not pass silently");
        assert.match(stderr, /fixture_finished/);
        assert.equal(await readFile(path.join(callerCodex, "auth.json"), "utf8"), "synthetic caller credential");
        const observed = JSON.parse(await readFile(report, "utf8"));
        assert.deepEqual(observed.overrides, [], "caller authentication/configuration must not reach the native child");
        for (const candidate of [observed.codexHome, observed.sqliteHome, observed.adapterLogs, observed.config.log_dir]) {
          assert.ok(candidate && !candidate.startsWith(callerHome + path.sep));
          assert.ok(path.relative(observed.cwd, candidate).split(path.sep)[0] !== "..");
        }
        assert.equal(observed.config.cli_auth_credentials_store, "file");
        assert.equal(observed.projectRoot, workspace, "keep the CLI's project workspace contract");
      } finally {
        await rm(root, { recursive: true, force: true });
      }
    });
}

const fixtureSource = `#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { createInterface } from "node:readline";
let projectRoot;
for await (const line of createInterface({ input: process.stdin })) {
  const request = JSON.parse(line);
  let result = {};
  let error;
  if (request.method === "settings/getAgentDetails") result = { agents: [{ agentId: "codex", authMethods: [{ id: "chat-gpt" }] }] };
  if (request.method === "project/add") { projectRoot = request.params.workspaceRoot; result = { project: { projectId: "project" } }; }
  if (request.method === "task/acquire") {
    const codexHome = process.env.CODEX_HOME || path.join(os.homedir(), ".codex");
    const logout = spawnSync(process.execPath, ["--input-type=module", "-e",
      'import { rmSync } from "node:fs"; import path from "node:path"; const home = process.env.CODEX_HOME || path.join(process.env.HOME, ".codex"); const roots = [process.env.OPENAIDE_PACKAGE_FIXTURE_ROOT, path.dirname(process.env.OPENAIDE_STORAGE_ROOT)]; if (!roots.some((root) => home.startsWith(root + path.sep))) throw new Error("fixture refused unowned home"); rmSync(path.join(home, "auth.json"), { force: true });'],
      { env: process.env, stdio: "pipe" });
    if (logout.status !== 0) throw new Error("fixture native child failed");
    writeFileSync(process.env.OPENAIDE_PACKAGE_FIXTURE_REPORT, JSON.stringify({
      codexHome, cwd: process.cwd(), projectRoot, sqliteHome: process.env.CODEX_SQLITE_HOME,
      adapterLogs: process.env.APP_SERVER_LOGS, config: JSON.parse(process.env.CODEX_CONFIG || "{}"),
      overrides: ["CODEX_PATH", "DEFAULT_AUTH_REQUEST", "MODEL_PROVIDER", "INITIAL_AGENT_MODE", "CODEX_API_KEY", "OPENAI_API_KEY", "CODEX_ACCESS_TOKEN"].filter((name) => process.env[name] !== undefined),
    }));
    result = { task: { task: { taskId: "prepared" } } };
  }
  if (request.method === "agent/listSessions") error = { error: { code: "fixture_finished", message: "fixture_finished" } };
  process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id: request.id, ...(error ? { error } : { result: { result } }) }) + "\\n");
}
`;
