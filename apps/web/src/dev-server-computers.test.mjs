import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import http from "node:http";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { listeningPort } from "./dev-server-test-support.mjs";

const DESK = "d".repeat(64);

test("each page reaches the computer it opened on", { timeout: 60_000 }, async (t) => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "openaide-web-computers-"));
  const staticRoot = path.join(fixtureRoot, "static");
  const fakeAppServerPath = path.join(fixtureRoot, "fake-app-server.mjs");
  const gatewayArguments = path.join(fixtureRoot, "gateway-arguments");
  const stateRoot = path.join(fixtureRoot, "state");
  mkdirSync(staticRoot);
  writeFileSync(path.join(staticRoot, "index.html"), "<html><body></body></html>");
  writeFileSync(fakeAppServerPath, fakeAppServerSource());
  chmodSync(fakeAppServerPath, 0o755);

  const webServer = spawn(process.execPath, ["src/dev-server.mjs"], {
    cwd: new URL("..", import.meta.url),
    env: {
      ...process.env,
      FAKE_GATEWAY_ARGUMENTS: gatewayArguments,
      OPENAIDE_APP_SERVER_PATH: fakeAppServerPath,
      OPENAIDE_WEB_ALLOWED_HOSTS: "localhost,127.0.0.1",
      OPENAIDE_WEB_HOST: "127.0.0.1",
      OPENAIDE_WEB_PORT: "0",
      OPENAIDE_WEB_RUNTIME_ROOT: path.join(fixtureRoot, "runtime"),
      OPENAIDE_WEB_STATE_ROOT: stateRoot,
      OPENAIDE_WEB_STATIC_ROOT: staticRoot,
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  t.after(async () => {
    await stopProcess(webServer);
    rmSync(fixtureRoot, { recursive: true, force: true });
  });
  const origin = `http://127.0.0.1:${await listeningPort(webServer)}`;
  const post = (route, body, cookie) => fetch(`${origin}${route}`, {
    method: "POST",
    headers: { "content-type": "application/json", origin, ...(cookie ? { cookie } : {}) },
    body: JSON.stringify(body),
  });
  const page = async (cookie) => (await fetch(origin, { headers: cookie ? { cookie } : {} })).text();

  assert.doesNotMatch(await page(), /data-computer-id/);

  const paired = await post("/__openaide-computers/pair", { code: "OAI1CODE" });
  assert.equal(paired.status, 200);
  assert.deepEqual(await paired.json(), { computer: { id: DESK, name: "Desk" } });
  const cookie = paired.headers.get("set-cookie").split(";", 1)[0];
  // The gateway keeps the Web App's key and list in a folder of its own.
  assert.deepEqual(JSON.parse(readFileSync(gatewayArguments, "utf8")), ["gateway", "--state", path.join(stateRoot, "web-app")]);

  const remotePage = await page(cookie);
  assert.match(remotePage, new RegExp(`data-computer-id="${DESK}"`));
  assert.match(remotePage, new RegExp(`/__openaide-app-server/computer/${DESK}/probe`));

  const remote = await post(`/__openaide-app-server/computer/${DESK}/probe`, rpc(), cookie);
  assert.equal(remote.status, 200);
  // The gateway's credential replaces the browser's, and this server's cookies stay here.
  assert.deepEqual((await remote.json()).result, { served: "remote", path: "/probe", cookie: null });
  // A page that opened on this computer stays on it, whatever the browser chose since.
  assert.equal((await (await post("/__openaide-app-server/probe", rpc(), cookie)).json()).result.served, "local");

  assert.equal(
    await socketEcho(origin, `/__openaide-app-server/computer/${DESK}/probe?connectionId=c1`, "hello"),
    "remote:/probe?connectionId=c1:hello",
  );

  const forgotten = await post("/__openaide-computers/forget", { computer: DESK }, cookie);
  assert.equal(forgotten.status, 200);
  assert.match(forgotten.headers.get("set-cookie"), /Max-Age=0/);
  const gone = await post(`/__openaide-app-server/computer/${DESK}/probe`, rpc(), cookie);
  assert.equal(gone.status, 503);
  assert.equal(await gone.text(), "That computer is no longer paired here.");
  assert.doesNotMatch(await page(cookie), /data-computer-id/);
});

function rpc() {
  return { jsonrpc: "2.0", id: "probe-1", method: "client/probe", params: {} };
}

/** Opens a socket through the Web server, sends `text`, and returns the first reply after the handshake. */
function socketEcho(origin, route, text) {
  return new Promise((resolve, reject) => {
    const request = http.request(`${origin}${route}`, {
      headers: {
        connection: "Upgrade",
        upgrade: "websocket",
        origin,
        "sec-websocket-key": "dGhlIHNhbXBsZSBub25jZQ==",
        "sec-websocket-version": "13",
      },
    });
    request.once("upgrade", (_response, socket) => {
      socket.once("data", (chunk) => {
        socket.destroy();
        resolve(chunk.toString("utf8"));
      });
      socket.write(text);
    });
    request.once("response", (response) => reject(new Error(`socket refused with ${response.statusCode}`)));
    request.once("error", reject);
    request.end();
  });
}

function stopProcess(child) {
  if (child.exitCode !== null) return Promise.resolve();
  return new Promise((resolve) => {
    child.once("exit", resolve);
    child.kill("SIGTERM");
  });
}

/**
 * This computer's App Server, or with `gateway` the gateway process, which serves
 * the paired computer `DESK` itself. Each answers with who served the request.
 */
function fakeAppServerSource() {
  return `#!/usr/bin/env node
import http from "node:http";
import { writeFileSync } from "node:fs";
import { createInterface } from "node:readline";

const DESK = "${DESK}";
const LOCAL_TOKEN = "test-token-that-is-long-enough-for-handoff";

function serve(name, token, listening) {
  const server = http.createServer((request, response) => {
    const chunks = [];
    request.on("data", (chunk) => chunks.push(chunk));
    request.on("end", () => {
      if (request.headers.authorization !== \`Bearer \${token}\`) {
        response.writeHead(401);
        response.end();
        return;
      }
      const message = JSON.parse(Buffer.concat(chunks).toString("utf8") || "{}");
      response.writeHead(200, { "content-type": "application/json" });
      response.end(JSON.stringify({
        jsonrpc: "2.0",
        id: message.id,
        result: { served: name, path: request.url, cookie: request.headers.cookie ?? null },
      }));
    });
  });
  server.on("upgrade", (request, socket) => {
    if (request.headers.authorization !== \`Bearer \${token}\`) {
      socket.destroy();
      return;
    }
    socket.write("HTTP/1.1 101 Switching Protocols\\r\\nUpgrade: websocket\\r\\nConnection: Upgrade\\r\\n\\r\\n");
    socket.on("data", (chunk) => socket.write(\`\${name}:\${request.url}:\${chunk}\`));
  });
  server.listen(0, "127.0.0.1", () => listening(server.address().port));
}

if (process.argv[2] === "gateway") {
  writeFileSync(process.env.FAKE_GATEWAY_ARGUMENTS, JSON.stringify(process.argv.slice(2)));
  let computers = [];
  serve("remote", "remote-token", (port) => {
    console.log(JSON.stringify({ event: "ready" }));
    createInterface({ input: process.stdin }).on("line", (line) => {
      const request = JSON.parse(line);
      const reply = (result) => console.log(JSON.stringify({ id: request.id, result }));
      if (request.method === "computers") reply({ computers });
      else if (request.method === "pair") {
        computers = [{ id: DESK, name: "Desk" }];
        reply({ computer: computers[0] });
      } else if (request.method === "connect") {
        if (computers.some((computer) => computer.id === request.computer)) {
          reply({ kind: "localHttp", endpointUrl: \`http://127.0.0.1:\${port}/probe\`, authToken: "remote-token" });
        } else {
          console.log(JSON.stringify({ id: request.id, error: { reason: "not_paired", message: "That computer is no longer paired here." } }));
        }
      } else if (request.method === "forget") {
        computers = [];
        reply({});
      }
    }).on("close", () => process.exit(0));
  });
} else {
  serve("local", LOCAL_TOKEN, (port) => {
    console.log(JSON.stringify({ kind: "localHttp", endpointUrl: \`http://127.0.0.1:\${port}/rpc\`, authToken: LOCAL_TOKEN }));
  });
  process.once("SIGTERM", () => process.exit(0));
}
`;
}
