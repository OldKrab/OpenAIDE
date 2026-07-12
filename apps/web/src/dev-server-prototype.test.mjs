import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import http from "node:http";
import net from "node:net";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";

test("Target proxies authenticated prototype HTTP and HMR traffic before the main app", { timeout: 8_000 }, async (t) => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "openaide-web-prototype-"));
  const staticRoot = path.join(fixtureRoot, "static");
  const fakeAppServerPath = path.join(fixtureRoot, "fake-app-server.mjs");
  mkdirSync(staticRoot);
  writeFileSync(path.join(staticRoot, "index.html"), "<html><body>Main app</body></html>");
  writeFileSync(fakeAppServerPath, fakeAppServerSource());
  chmodSync(fakeAppServerPath, 0o755);

  const prototypeServer = http.createServer((request, response) => {
    response.writeHead(200, { "content-type": "text/plain", etag: "prototype-etag" });
    response.end(`prototype:${request.url}`);
  });
  prototypeServer.on("upgrade", (request, socket) => {
    socket.end([
      "HTTP/1.1 101 Switching Protocols",
      "Connection: Upgrade",
      "Upgrade: websocket",
      "",
      `hmr:${request.url}`,
    ].join("\r\n"));
  });
  const prototypePort = await listenOnAvailablePort(prototypeServer);
  const webPort = await availablePort();
  const webServer = spawn(process.execPath, ["src/dev-server.mjs"], {
    cwd: new URL("..", import.meta.url),
    env: {
      ...process.env,
      OPENAIDE_APP_SERVER_PATH: fakeAppServerPath,
      OPENAIDE_WEB_ALLOWED_HOSTS: "localhost,127.0.0.1",
      OPENAIDE_WEB_HOST: "127.0.0.1",
      OPENAIDE_WEB_PORT: String(webPort),
      OPENAIDE_WEB_PROTOTYPE_PORT: String(prototypePort),
      OPENAIDE_WEB_RUNTIME_ROOT: path.join(fixtureRoot, "runtime"),
      OPENAIDE_WEB_STATE_ROOT: path.join(fixtureRoot, "state"),
      OPENAIDE_WEB_STATIC_ROOT: staticRoot,
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  t.after(async () => {
    await Promise.all([stopProcess(webServer), closeServer(prototypeServer)]);
    rmSync(fixtureRoot, { recursive: true, force: true });
  });
  await waitForOutput(webServer, "OpenAIDE Web dev shell listening");

  const response = await fetch(`http://127.0.0.1:${webPort}/prototype/example/?variant=B`);
  assert.equal(response.status, 200);
  assert.equal(await response.text(), "prototype:/prototype/example/?variant=B");
  assert.equal(response.headers.get("cache-control"), "no-store, max-age=0, must-revalidate");
  assert.equal(response.headers.get("etag"), null);

  const upgrade = await upgradeRequest(webPort, "/prototype/hmr?token=test");
  assert.match(upgrade, /101 Switching Protocols/);
  assert.match(upgrade, /hmr:\/prototype\/hmr\?token=test/);

  await closeServer(prototypeServer);
  const unavailable = await fetch(`http://127.0.0.1:${webPort}/prototype/example/`);
  assert.equal(unavailable.status, 503);
  assert.equal(await unavailable.text(), "Prototype server is not running. Start it with npm run prototype:target.");
});

function upgradeRequest(port, requestPath) {
  return new Promise((resolve, reject) => {
    const socket = net.connect(port, "127.0.0.1");
    let response = "";
    let settled = false;
    socket.setEncoding("utf8");
    socket.once("connect", () => {
      socket.write([
        `GET ${requestPath} HTTP/1.1`,
        `Host: 127.0.0.1:${port}`,
        `Origin: http://127.0.0.1:${port}`,
        "Connection: Upgrade",
        "Upgrade: websocket",
        "Sec-WebSocket-Version: 13",
        "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==",
        "Sec-WebSocket-Protocol: vite-hmr",
        "",
        "",
      ].join("\r\n"));
    });
    socket.on("data", (chunk) => {
      response += chunk;
      if (response.includes("hmr:")) {
        settled = true;
        resolve(response);
        socket.destroy();
      }
    });
    socket.once("end", () => {
      if (!settled) resolve(response);
    });
    socket.once("error", reject);
  });
}

function availablePort() {
  const server = http.createServer();
  return listenOnAvailablePort(server).finally(() => closeServer(server));
}

function listenOnAvailablePort(server) {
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => resolve(server.address().port));
  });
}

function closeServer(server) {
  if (!server.listening) return Promise.resolve();
  return new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
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
    child.stderr.on("data", (chunk) => { stderr += chunk; });
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
    const message = JSON.parse(Buffer.concat(chunks).toString("utf8"));
    response.writeHead(request.headers.authorization === \`Bearer \${authToken}\` ? 200 : 401, { "content-type": "application/json" });
    response.end(JSON.stringify({ jsonrpc: "2.0", id: message.id, result: {} }));
  });
});
server.listen(0, "127.0.0.1", () => console.log(JSON.stringify({
  kind: "localHttp",
  endpointUrl: \`http://127.0.0.1:\${server.address().port}/rpc\`,
  authToken,
})));
process.once("SIGTERM", () => server.close(() => process.exit(0)));
`;
}
