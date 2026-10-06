import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { listeningPort } from "./dev-server-test-support.mjs";

// Bounds a hung wait; never sized to how long startup usually takes.
const WATCHDOG_MS = 30_000;

test("Web stays live while a slow App Server handoff becomes ready", { timeout: 60_000 }, async (t) => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "openaide-web-startup-"));
  const staticRoot = path.join(fixtureRoot, "static");
  const fakeAppServerPath = path.join(fixtureRoot, "slow-app-server.mjs");
  mkdirSync(staticRoot);
  writeFileSync(path.join(staticRoot, "index.html"), "<html><body>OpenAIDE starting</body></html>");
  const handoffGate = path.join(fixtureRoot, "handoff-gate");
  writeFileSync(handoffGate, "");
  writeFileSync(fakeAppServerPath, slowAppServerSource(handoffGate));
  chmodSync(fakeAppServerPath, 0o755);

  const webServer = spawn(process.execPath, ["src/dev-server.mjs"], {
    cwd: new URL("..", import.meta.url),
    env: {
      ...process.env,
      OPENAIDE_APP_SERVER_PATH: fakeAppServerPath,
      OPENAIDE_WEB_ALLOWED_HOSTS: "localhost,127.0.0.1",
      OPENAIDE_WEB_HOST: "127.0.0.1",
      OPENAIDE_WEB_PORT: "0",
      OPENAIDE_WEB_RUNTIME_ROOT: path.join(fixtureRoot, "runtime"),
      OPENAIDE_WEB_STATE_ROOT: path.join(fixtureRoot, "state"),
      OPENAIDE_WEB_STATIC_ROOT: staticRoot,
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  t.after(async () => {
    await stopProcess(webServer);
    rmSync(fixtureRoot, { recursive: true, force: true });
  });

  const port = await listeningPort(webServer);
  const origin = `http://127.0.0.1:${port}`;
  const live = await fetch(`${origin}/livez`);
  const starting = await fetch(`${origin}/readyz`);
  const page = await fetch(origin);

  assert.equal(live.status, 200);
  assert.equal(await live.text(), "live");
  assert.equal(starting.status, 503);
  assert.equal(await starting.text(), "starting");
  assert.equal(page.status, 200);
  assert.match(await page.text(), /OpenAIDE starting/);

  rmSync(handoffGate);
  await waitUntilReady(`${origin}/readyz`, WATCHDOG_MS);
});

test("Web survives a malformed handoff and can retry a repaired App Server", { timeout: 60_000 }, async (t) => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "openaide-web-invalid-handoff-"));
  const staticRoot = path.join(fixtureRoot, "static");
  const fakeAppServerPath = path.join(fixtureRoot, "app-server.mjs");
  mkdirSync(staticRoot);
  writeFileSync(path.join(staticRoot, "index.html"), "<html><body>OpenAIDE recovery</body></html>");
  writeFileSync(fakeAppServerPath, '#!/usr/bin/env node\nconsole.log("invalid handoff");\n');
  chmodSync(fakeAppServerPath, 0o755);

  const webServer = spawn(process.execPath, ["src/dev-server.mjs"], {
    cwd: new URL("..", import.meta.url),
    env: {
      ...process.env,
      OPENAIDE_APP_SERVER_PATH: fakeAppServerPath,
      OPENAIDE_WEB_ALLOWED_HOSTS: "localhost,127.0.0.1",
      OPENAIDE_WEB_HOST: "127.0.0.1",
      OPENAIDE_WEB_PORT: "0",
      OPENAIDE_WEB_RUNTIME_ROOT: path.join(fixtureRoot, "runtime"),
      OPENAIDE_WEB_STATE_ROOT: path.join(fixtureRoot, "state"),
      OPENAIDE_WEB_STATIC_ROOT: staticRoot,
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  t.after(async () => {
    await stopProcess(webServer);
    rmSync(fixtureRoot, { recursive: true, force: true });
  });

  const [port] = await Promise.all([
    listeningPort(webServer),
    waitForOutput(webServer, "app_server_handoff_failed"),
  ]);
  const origin = `http://127.0.0.1:${port}`;
  assert.equal((await fetch(`${origin}/livez`)).status, 200);
  assert.match(await (await fetch(origin)).text(), /OpenAIDE recovery/);

  writeFileSync(fakeAppServerPath, slowAppServerSource(path.join(fixtureRoot, "open-gate"), 0));
  await waitUntilReady(`${origin}/readyz`, WATCHDOG_MS);
});

function waitForOutput(child, expected) {
  return new Promise((resolve, reject) => {
    let stdout = "";
    let stderr = "";
    const timeout = setTimeout(() => reject(new Error(`Web did not print "${expected}": ${stderr}`)), WATCHDOG_MS);
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
      if (!stdout.includes(expected)) return;
      clearTimeout(timeout);
      resolve();
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
      if (!stderr.includes(expected)) return;
      clearTimeout(timeout);
      resolve();
    });
    child.once("error", (error) => {
      clearTimeout(timeout);
      reject(error);
    });
    child.once("exit", (code) => {
      clearTimeout(timeout);
      reject(new Error(`Web server exited with ${code}: ${stderr}`));
    });
  });
}

async function waitUntilReady(url, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const response = await fetch(url);
    if (response.status === 200) {
      assert.equal(await response.text(), "ready");
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 50)); // timing: poll
  }
  throw new Error(`Web did not become ready within ${timeoutMs}ms`);
}

function stopProcess(child) {
  if (child.exitCode !== null) return Promise.resolve();
  return new Promise((resolve) => {
    child.once("exit", resolve);
    child.kill("SIGTERM");
  });
}

// Holds the handoff while the gate file exists, so "starting" is observed at a
// gate. The delay keeps the handoff later than the shell heartbeat interval.
function slowAppServerSource(gate, delayMs = 5_500) {
  return `#!/usr/bin/env node
import { existsSync } from "node:fs";
import http from "node:http";
const authToken = "test-token-that-is-long-enough-for-handoff";
const server = http.createServer((request, response) => {
  const chunks = [];
  request.on("data", (chunk) => chunks.push(chunk));
  request.on("end", () => {
    const message = JSON.parse(Buffer.concat(chunks).toString("utf8"));
    response.writeHead(200, { "content-type": "application/json" });
    response.end(JSON.stringify({ jsonrpc: "2.0", id: message.id, result: {} }));
  });
});
// timing: contract — the handoff arrives later than the shell heartbeat interval.
await new Promise((resolve) => setTimeout(resolve, ${delayMs}));
// timing: poll
while (existsSync(${JSON.stringify(gate)})) await new Promise((resolve) => setTimeout(resolve, 10));
server.listen(0, "127.0.0.1", () => console.log(JSON.stringify({
  kind: "localHttp",
  endpointUrl: \`http://127.0.0.1:\${server.address().port}/rpc\`,
  authToken,
})));
process.once("SIGTERM", () => server.close(() => process.exit(0)));
`;
}
