#!/usr/bin/env node
import { spawn } from "node:child_process";
import { access, mkdtemp, readFile, readdir, rm, stat } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";

const REQUIRED_PACKAGE_FILES = [
  "LICENSE",
  "README.md",
  "openaide-web.service",
  "bin/openaide-web",
  "bin/openaide-app-server",
  "apps/web/src/server.mjs",
  "node_modules/@openaide/app-server-client/dist/index.js",
  "packages/frontend/dist/index.html",
  "packages/frontend/dist/assets/index.js",
  "packages/frontend/dist/assets/index.css",
  "packages/frontend/dist/mermaid-renderer.html",
  "packages/frontend/dist/mermaid-renderer.js",
];
// Bounds a hung start; a healthy package answers in a few seconds.
const WATCHDOG_MS = 60_000;

/**
 * Verifies the unpacked Web App archive and starts it from its own files: the
 * launcher, the production server, and the bundled App Server must serve the
 * Frontend and answer a protocol request through the browser-facing proxy.
 * With an ACP Agent fixture, the packaged stack must also complete one Task.
 */
export async function smokeReleaseWeb({ agentFixture, packageRoot, version }) {
  const manifest = JSON.parse(await readFile(path.join(packageRoot, "package.json"), "utf8"));
  if (manifest.name !== "openaide-web" || manifest.version !== version) {
    throw new Error(`Web package identity ${manifest.name}@${manifest.version} does not match openaide-web@${version}`);
  }
  for (const relativePath of REQUIRED_PACKAGE_FILES) {
    await access(path.join(packageRoot, relativePath));
  }
  for (const executable of ["bin/openaide-web", "bin/openaide-app-server"]) {
    if (((await stat(path.join(packageRoot, executable))).mode & 0o111) === 0) {
      throw new Error(`${executable} is not executable`);
    }
  }
  const shipped = await readdir(packageRoot, { recursive: true });
  const unexpected = shipped.filter((entry) => /\.map$|[.-]test\.mjs$/.test(entry));
  if (unexpected.length > 0) {
    throw new Error(`Web package contains development files: ${unexpected.slice(0, 5).join(", ")}`);
  }

  const root = await mkdtemp(path.join(os.tmpdir(), "openaide-web-release-smoke-"));
  // The package must supply every setting it needs; inherited overrides would hide a broken default.
  const env = Object.fromEntries(Object.entries(process.env).filter(([name]) => !name.startsWith("OPENAIDE_")));
  const server = spawn(path.join(packageRoot, "bin/openaide-web"), [], {
    cwd: root,
    env: {
      ...env,
      OPENAIDE_WEB_PORT: "0",
      OPENAIDE_WEB_PROJECT_ROOTS: root,
      XDG_DATA_HOME: path.join(root, "data"),
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  let stderr = "";
  server.stderr.setEncoding("utf8").on("data", (chunk) => { stderr += chunk; });
  try {
    const origin = await withWatchdog(listeningOrigin(server), "listen");
    await withWatchdog(untilReady(origin), "report App Server readiness");

    const page = await fetch(origin);
    if (page.status !== 200 || !/data-shell="web"/.test(await page.text())) {
      throw new Error(`Frontend document was not served with the Web bootstrap (HTTP ${page.status})`);
    }
    const script = await fetch(`${origin}/assets/index.js`, { headers: { "accept-encoding": "br" } });
    if (script.status !== 200 || script.headers.get("content-encoding") !== "br" || !script.headers.get("etag")) {
      throw new Error("Frontend script was not served compressed with a validator");
    }
    await script.arrayBuffer();

    const request = protocolClient(origin);
    const initialized = await request("client/initialize", {
      clientInstanceId: "release-smoke",
      shell: { kind: "web" },
      requestedSurface: { kind: "home" },
      capabilities: { protocol: ["requestResponses", "stableClientRequestIds", "resync"], shell: [] },
    });
    if (!initialized.snapshot?.server) {
      throw new Error("Bundled App Server did not return its snapshot through the proxy");
    }
    await access(path.join(root, "data/openaide-web/state"));
    if (agentFixture) {
      await withWatchdog(completeTask(request, initialized.snapshot, agentFixture), "complete a Task");
    }
  } catch (error) {
    throw new Error(`${error instanceof Error ? error.message : String(error)}\n${stderr.slice(-2000)}`);
  } finally {
    if (server.exitCode === null) {
      await new Promise((resolve) => {
        server.once("exit", resolve);
        server.kill("SIGTERM");
      });
    }
    await rm(root, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
  }
}

function protocolClient(origin) {
  let nextRequestId = 0;
  return async (method, params) => {
    const id = `release-smoke-${++nextRequestId}`;
    const response = await fetch(`${origin}/__openaide-app-server/probe`, {
      method: "POST",
      headers: { "content-type": "application/json", "x-openaide-connection-id": "release-smoke" },
      body: JSON.stringify({ jsonrpc: "2.0", id, method, params }),
    });
    if (response.status !== 200) throw new Error(`${method} was answered with HTTP ${response.status}`);
    const message = [await response.json()].flat().find((candidate) => candidate?.id === id);
    if (!message || message.error) {
      throw new Error(`${method} failed: ${message?.error?.code ?? "no response"}`);
    }
    return message.result?.result ?? message.result;
  };
}

/** Sends one prompt to the fixture Agent and requires its reply in an idle Task. */
async function completeTask(request, snapshot, agentFixture) {
  const projectId = snapshot.projects?.activeProjectId ?? snapshot.projects?.projects?.[0]?.projectId;
  if (!projectId) throw new Error("The packaged server did not offer its configured Project");
  const agentId = "custom.openaide-release-smoke";
  await request("agent/createCustom", {
    agentId,
    label: "OpenAIDE Release Smoke Agent",
    icon: "terminal",
    commandLine: [process.execPath, agentFixture].join(" "),
    command: process.execPath,
    args: [agentFixture],
    env: {},
    secretEnv: [],
    enabled: true,
  });
  let { task } = await request("task/acquire", { projectId, agentId });
  const taskId = task?.task?.taskId;
  if (!taskId) throw new Error("task/acquire returned no Task");
  const reopen = async () => {
    await new Promise((resolve) => setTimeout(resolve, 200));
    ({ task } = await request("task/open", { taskId }));
  };
  while (task.sendCapability?.state !== "ready") await reopen();
  await request("task/send", { taskId, message: { text: "OpenAIDE release smoke", attachments: [] } });
  const replied = () => task.task.status === "idle"
    && task.chat?.items?.some((item) => item.role === "agent" && item.status === "complete");
  while (!replied()) {
    if (["failed", "interrupted"].includes(task.task.status)) {
      throw new Error(`The Task ended as ${task.task.status} instead of completing`);
    }
    await reopen();
  }
}

function listeningOrigin(child) {
  return new Promise((resolve, reject) => {
    let stdout = "";
    child.stdout.setEncoding("utf8").on("data", (chunk) => {
      stdout += chunk;
      const listening = / listening on (http:\/\/[^\s:]+:\d+)\r?\n/.exec(stdout);
      if (listening) resolve(listening[1]);
    });
    child.once("error", reject);
    child.once("exit", (code) => reject(new Error(`Packaged Web server exited with ${code} before listening`)));
  });
}

async function untilReady(origin) {
  for (;;) {
    if ((await fetch(`${origin}/readyz`)).status === 200) return;
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
}

function withWatchdog(promise, action) {
  let timeout;
  return Promise.race([
    promise,
    new Promise((_, reject) => {
      timeout = setTimeout(() => reject(new Error(`Packaged Web server did not ${action} within ${WATCHDOG_MS}ms`)), WATCHDOG_MS);
    }),
  ]).finally(() => clearTimeout(timeout));
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  const [packageRoot, version, agentFixture] = process.argv.slice(2);
  if (!packageRoot || !version) {
    console.error("Usage: node scripts/smoke-release-web.mjs <unpacked-package-root> <version> [acp-agent-fixture]");
    process.exit(2);
  }
  await smokeReleaseWeb({
    agentFixture: agentFixture ? path.resolve(agentFixture) : undefined,
    packageRoot: path.resolve(packageRoot),
    version,
  });
  console.log(`OpenAIDE Web ${version} package smoke passed.`);
}
