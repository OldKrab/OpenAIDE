import { spawn } from "node:child_process";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { listeningPort } from "../../apps/web/src/dev-server-test-support.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const defaultAgentFixture = path.join(repoRoot, "tests/smoke/fixtures/test-acp-agent.mjs");

/** Starts an isolated real Web, App Server, and deterministic ACP Agent stack. */
export async function startFullStackHarness({ agentArgs = [], frontend = "web", agentFixture = defaultAgentFixture, webTransport } = {}) {
  const root = await mkdtemp(path.join(os.tmpdir(), "openaide-smoke-"));
  const staticRoot = path.join(root, "static");
  // Tests order the Agent fixture through gate files here instead of delays.
  const gateRoot = path.join(root, "gates");
  await mkdir(gateRoot);
  try {
    await run("npm", ["run", "build:typescript-deps"]);
    if (frontend === "desktop") {
      await run("npm", [
        "exec",
        "--workspace",
        "openaide-frontend",
        "vite",
        "--",
        "build",
        "--config",
        path.join(repoRoot, "tests/smoke/desktop-shell/vite.config.mjs"),
        "--outDir",
        staticRoot,
        "--emptyOutDir",
      ]);
    } else {
      await run("npm", [
        "run",
        "build",
        "--workspace",
        "openaide-frontend",
        "--",
        "--outDir",
        staticRoot,
        "--emptyOutDir",
      ]);
    }
    await run("cargo", ["build", "-p", "openaide-app-server"]);
  } catch (error) {
    await rm(root, { recursive: true, force: true });
    throw error;
  }

  const logs = [];
  const server = spawn(process.execPath, [path.join(repoRoot, "apps/web/src/dev-server.mjs")], {
    cwd: repoRoot,
    env: {
      ...environmentWithoutOpenAideState(),
      OPENAIDE_APP_SERVER_PATH: path.resolve(repoRoot, process.env.CARGO_TARGET_DIR ?? "target", "debug/openaide-app-server"),
      OPENAIDE_WEB_ALLOWED_HOSTS: "localhost,127.0.0.1",
      OPENAIDE_WEB_HOST: "127.0.0.1",
      OPENAIDE_WEB_PORT: "0",
      OPENAIDE_WEB_PROJECT_ROOTS: repoRoot,
      OPENAIDE_WEB_RUNTIME_ROOT: path.join(root, "runtime"),
      OPENAIDE_WEB_STATE_ROOT: path.join(root, "state"),
      OPENAIDE_WEB_STATIC_ROOT: staticRoot,
      ...(webTransport ? { OPENAIDE_WEB_TRANSPORT: webTransport } : {}),
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  const listening = listeningPort(server);
  capture(server.stdout, logs, "web");
  capture(server.stderr, logs, "web:error");

  let baseUrl;
  try {
    baseUrl = `http://127.0.0.1:${await listening}`;
    await waitForServer(baseUrl, server, logs);
    const setup = createProbeClient(baseUrl, `smoke-setup-${Date.now()}`);
    const initialized = await setup.request("client/initialize", {
      clientInstanceId: `smoke-setup-${Date.now()}`,
      shell: { kind: "web", name: "full-stack-smoke-setup" },
      requestedSurface: { kind: "home" },
      capabilities: { protocol: ["requestResponses", "stableClientRequestIds", "resync"], shell: [] },
    });
    await setup.request("agent/createCustom", {
      agentId: "custom.openaide-smoke-agent",
      label: "OpenAIDE Test Agent",
      icon: "terminal",
      commandLine: [process.execPath, agentFixture, ...agentArgs].join(" "),
      command: process.execPath,
      args: [agentFixture, ...agentArgs],
      env: { OPENAIDE_SMOKE_GATE_ROOT: gateRoot },
      secretEnv: [],
      enabled: true,
    });
    for (const agent of initialized.snapshot?.agents?.agents ?? []) {
      await setup.request("agent/setEnabled", { agentId: agent.agentId, enabled: false });
    }
    if (agentArgs.includes("--active-writer")) {
      const projectId = initialized.snapshot?.projects?.projects?.[0]?.projectId;
      if (!projectId) throw new Error("Desktop Native Session smoke scenario requires a Project");
      await setup.request("agent/listSessions", {
        agentId: "custom.openaide-smoke-agent",
        projectId,
        cursor: null,
      });
    }
  } catch (error) {
    await stopProcess(server);
    await rm(root, { recursive: true, force: true });
    throw new Error(`${error.message}\n${logs.join("\n")}`);
  }

  return {
    baseUrl,
    stateRoot: path.join(root, "state"),
    logs,
    /** Closes the named gate; the Agent fixture holds wherever it awaits it. */
    async hold(gate) {
      await writeFile(path.join(gateRoot, gate), "");
    },
    /** Opens the named gate. */
    async release(gate) {
      await rm(path.join(gateRoot, gate), { force: true });
    },
    async close() {
      await stopProcess(server);
      await rm(root, { recursive: true, force: true });
    },
  };
}

function environmentWithoutOpenAideState() {
  // A smoke stack must never inherit Driver/Target roots, ports, auth, or presentation.
  return Object.fromEntries(
    Object.entries(process.env).filter(([name]) => !name.startsWith("OPENAIDE_")),
  );
}

function createProbeClient(baseUrl, connectionId) {
  let nextId = 1;
  return {
    async request(method, params) {
      const id = `smoke-probe-${nextId++}`;
      const response = await fetch(`${baseUrl}/__openaide-app-server/probe`, {
        method: "POST",
        headers: {
          "content-type": "application/json",
          "x-openaide-connection-id": connectionId,
        },
        body: JSON.stringify({ jsonrpc: "2.0", id, method, params }),
      });
      const body = await response.json();
      const messages = Array.isArray(body) ? body : [body];
      const result = messages.find((message) => message.id === id);
      if (!response.ok || result?.error) {
        throw new Error(`${method} failed (${response.status}): ${result?.error?.message ?? JSON.stringify(body).slice(0, 1_000)}`);
      }
      if (!result) {
        throw new Error(`${method} returned no matching response: ${JSON.stringify(body).slice(0, 1_000)}`);
      }
      return result?.result?.result ?? result?.result;
    },
  };
}

async function run(command, args) {
  const child = spawn(command, args, {
    cwd: repoRoot,
    env: environmentWithoutOpenAideState(),
    stdio: ["ignore", "pipe", "pipe"],
  });
  let output = "";
  child.stdout.on("data", (chunk) => { output += chunk; });
  child.stderr.on("data", (chunk) => { output += chunk; });
  const code = await new Promise((resolve, reject) => {
    child.once("error", reject);
    child.once("exit", resolve);
  });
  if (code !== 0) throw new Error(`${command} ${args.join(" ")} failed (${code}):\n${output}`);
}

async function waitForServer(baseUrl, child, logs) {
  const deadline = Date.now() + 30_000;
  let lastResponse;
  while (Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error(`Web server exited with ${child.exitCode}`);
    try {
      const response = await fetch(`${baseUrl}/new-task`);
      if (response.ok) return;
      lastResponse = `HTTP ${response.status}: ${(await response.text()).slice(0, 500)}`;
    } catch {
      // The Vite and App Server children become ready independently.
    }
    await new Promise((resolve) => setTimeout(resolve, 100)); // timing: poll
  }
  throw new Error(`Web server did not become ready${lastResponse ? ` (${lastResponse})` : ""}.`);
}

function capture(stream, logs, prefix) {
  stream.setEncoding("utf8");
  stream.on("data", (chunk) => {
    for (const line of chunk.split(/\r?\n/)) {
      if (line) logs.push(`[${prefix}] ${line}`);
    }
  });
}

async function stopProcess(child) {
  if (child.exitCode !== null) return;
  child.kill("SIGTERM");
  await Promise.race([
    new Promise((resolve) => child.once("exit", resolve)),
    // timing: expiry — teardown grace before SIGKILL; no assertion depends on it.
    new Promise((resolve) => setTimeout(resolve, 5_000)),
  ]);
  if (child.exitCode === null) child.kill("SIGKILL");
}
