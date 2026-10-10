import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { listeningPort } from "./dev-server-test-support.mjs";

test("the packaged server serves the Frontend and keeps user data outside the install", { timeout: 60_000 }, async (t) => {
  const fixture = createFixture(t);
  const webServer = startServer(t, fixture, { OPENAIDE_WEB_HOST: "127.0.0.1" });
  const origin = `http://127.0.0.1:${await listeningPort(webServer)}`;

  assert.equal((await fetch(`${origin}/livez`)).status, 200);
  const page = await fetch(origin);
  assert.equal(page.status, 200);
  assert.match(await page.text(), /data-shell="web"/);
  // Prototype routing belongs to the development server only.
  assert.equal((await fetch(`${origin}/prototype/example`)).status, 404);

  const rpc = await fetch(`${origin}/__openaide-app-server/probe`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: "probe", method: "client/probe", params: {} }),
  });
  assert.equal(rpc.status, 200);
  assert.ok(existsSync(path.join(fixture.dataHome, "openaide-web", "state")));
  assert.ok(existsSync(path.join(fixture.dataHome, "openaide-web", "runtime")));
});

test("a network-reachable bind requires a named protection", { timeout: 60_000 }, async (t) => {
  const fixture = createFixture(t);

  const refused = startServer(t, fixture, { OPENAIDE_WEB_HOST: "0.0.0.0" });
  let stderr = "";
  refused.stderr.setEncoding("utf8");
  refused.stderr.on("data", (chunk) => { stderr += chunk; });
  const exitCode = await new Promise((resolve) => refused.once("exit", resolve));
  assert.notEqual(exitCode, 0);
  assert.match(stderr, /Refusing to listen on 0\.0\.0\.0 without authentication/);

  const upstream = startServer(t, fixture, { OPENAIDE_WEB_HOST: "0.0.0.0", OPENAIDE_WEB_UPSTREAM_AUTH: "1" });
  const upstreamPort = await listeningPort(upstream);
  assert.equal((await fetch(`http://127.0.0.1:${upstreamPort}/livez`)).status, 200);

  const password = startServer(t, fixture, { OPENAIDE_WEB_HOST: "0.0.0.0", OPENAIDE_WEB_PASSWORD: "fixture-password" });
  const passwordPort = await listeningPort(password);
  assert.equal((await fetch(`http://127.0.0.1:${passwordPort}/`)).status, 401);
});

function createFixture(t) {
  const root = mkdtempSync(path.join(tmpdir(), "openaide-web-server-"));
  const staticRoot = path.join(root, "static");
  const appServerPath = path.join(root, "app-server.mjs");
  mkdirSync(staticRoot);
  writeFileSync(path.join(staticRoot, "index.html"), "<html><body><div id=\"root\"></div></body></html>");
  writeFileSync(appServerPath, fakeAppServerSource());
  chmodSync(appServerPath, 0o755);
  t.after(() => rmSync(root, { recursive: true, force: true }));
  return { appServerPath, dataHome: path.join(root, "data"), staticRoot };
}

function startServer(t, fixture, env) {
  const {
    OPENAIDE_WEB_PASSWORD: _password,
    OPENAIDE_WEB_RUNTIME_ROOT: _runtimeRoot,
    OPENAIDE_WEB_STATE_ROOT: _stateRoot,
    OPENAIDE_WEB_UPSTREAM_AUTH: _upstreamAuth,
    ...baseEnv
  } = process.env;
  const child = spawn(process.execPath, ["src/server.mjs"], {
    cwd: new URL("..", import.meta.url),
    env: {
      ...baseEnv,
      OPENAIDE_APP_SERVER_PATH: fixture.appServerPath,
      OPENAIDE_WEB_PORT: "0",
      OPENAIDE_WEB_STATIC_ROOT: fixture.staticRoot,
      XDG_DATA_HOME: fixture.dataHome,
      ...env,
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  t.after(() => {
    if (child.exitCode !== null) return undefined;
    return new Promise((resolve) => {
      child.once("exit", resolve);
      child.kill("SIGTERM");
    });
  });
  return child;
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
    response.writeHead(200, { "content-type": "application/json" });
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
