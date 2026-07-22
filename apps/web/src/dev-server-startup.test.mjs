import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import http from "node:http";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";

test("Web stays live while a slow App Server handoff becomes ready", { timeout: 12_000 }, async (t) => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "openaide-web-startup-"));
  const staticRoot = path.join(fixtureRoot, "static");
  const fakeAppServerPath = path.join(fixtureRoot, "slow-app-server.mjs");
  mkdirSync(staticRoot);
  writeFileSync(path.join(staticRoot, "index.html"), "<html><body>OpenAIDE starting</body></html>");
  writeFileSync(fakeAppServerPath, slowAppServerSource());
  chmodSync(fakeAppServerPath, 0o755);

  const port = await availablePort();
  const webServer = spawn(process.execPath, ["src/dev-server.mjs"], {
    cwd: new URL("..", import.meta.url),
    env: {
      ...process.env,
      OPENAIDE_APP_SERVER_PATH: fakeAppServerPath,
      OPENAIDE_WEB_ALLOWED_HOSTS: "localhost,127.0.0.1",
      OPENAIDE_WEB_HOST: "127.0.0.1",
      OPENAIDE_WEB_PORT: String(port),
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

  await waitForOutput(webServer, "OpenAIDE Web dev shell listening", 2_000);
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

  await waitUntilReady(`${origin}/readyz`, 8_000);
});

function availablePort() {
  const server = http.createServer();
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      server.close((error) => error ? reject(error) : resolve(address.port));
    });
  });
}

function waitForOutput(child, expected, timeoutMs) {
  return new Promise((resolve, reject) => {
    let stdout = "";
    let stderr = "";
    const timeout = setTimeout(() => reject(new Error(`Web did not listen within ${timeoutMs}ms: ${stderr}`)), timeoutMs);
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
      if (!stdout.includes(expected)) return;
      clearTimeout(timeout);
      resolve();
    });
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.once("error", reject);
    child.once("exit", (code) => reject(new Error(`Web server exited with ${code}: ${stderr}`)));
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
    await new Promise((resolve) => setTimeout(resolve, 50));
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

function slowAppServerSource() {
  return `#!/usr/bin/env node
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
setTimeout(() => server.listen(0, "127.0.0.1", () => console.log(JSON.stringify({
  kind: "localHttp",
  endpointUrl: \`http://127.0.0.1:\${server.address().port}/rpc\`,
  authToken,
}))), 5_500);
process.once("SIGTERM", () => server.close(() => process.exit(0)));
`;
}
