import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import http from "node:http";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";

test("App Server proxy accepts only the exact browser origin", { timeout: 5_000 }, async (t) => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "openaide-web-origin-"));
  const staticRoot = path.join(fixtureRoot, "static");
  const fakeAppServerPath = path.join(fixtureRoot, "fake-app-server.mjs");
  mkdirSync(staticRoot);
  writeFileSync(path.join(staticRoot, "index.html"), "<html><body>OpenAIDE</body></html>");
  writeFileSync(fakeAppServerPath, fakeAppServerSource());
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
  await waitForOutput(webServer, "OpenAIDE Web dev shell listening");

  const endpoint = `http://127.0.0.1:${port}/__openaide-app-server/probe`;
  const unrelatedPort = port === 65_535 ? port - 1 : port + 1;
  const rejected = await proxyRequest(endpoint, `http://127.0.0.1:${unrelatedPort}`, "rejected");
  const accepted = await proxyRequest(endpoint, `http://127.0.0.1:${port}`, "accepted");

  assert.equal(rejected.status, 403);
  assert.equal(await rejected.text(), "Origin not allowed");
  assert.equal(accepted.status, 200);
});

function proxyRequest(endpoint, origin, id) {
  return fetch(endpoint, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      origin,
    },
    body: JSON.stringify({ jsonrpc: "2.0", id, method: "client/probe", params: {} }),
  });
}

function availablePort() {
  const server = http.createServer();
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      server.close((error) => {
        if (error) reject(error);
        else resolve(address.port);
      });
    });
  });
}

function waitForOutput(child, expected) {
  return new Promise((resolve, reject) => {
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
      if (stdout.includes(expected)) resolve();
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
    });
    child.once("error", reject);
    child.once("exit", (code) => reject(new Error(`Web server exited with ${code}: ${stderr}`)));
  });
}

function stopProcess(child) {
  if (child.exitCode !== null) return Promise.resolve();
  return new Promise((resolve) => {
    child.once("exit", resolve);
    child.kill("SIGTERM");
  });
}

function fakeAppServerSource() {
  return `#!/usr/bin/env node
import http from "node:http";

const authToken = "test-token-that-is-long-enough-for-handoff";
const server = http.createServer((request, response) => {
  const chunks = [];
  request.on("data", (chunk) => chunks.push(chunk));
  request.on("end", () => {
    if (request.headers.authorization !== \`Bearer \${authToken}\`) {
      response.writeHead(401);
      response.end("missing proxy credential");
      return;
    }
    const message = JSON.parse(Buffer.concat(chunks).toString("utf8"));
    response.writeHead(200, { "content-type": "application/json" });
    response.end(JSON.stringify({ jsonrpc: "2.0", id: message.id, result: {} }));
  });
});
server.listen(0, "127.0.0.1", () => {
  const port = server.address().port;
  console.log(JSON.stringify({
    kind: "localHttp",
    endpointUrl: \`http://127.0.0.1:\${port}/rpc\`,
    authToken,
  }));
});
process.once("SIGTERM", () => server.close(() => process.exit(0)));
`;
}
