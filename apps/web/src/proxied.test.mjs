import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import http from "node:http";
import https from "node:https";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { listeningPort } from "./dev-server-test-support.mjs";

const PUBLIC_HOST = "openaide.test";

// The supported remote topology: the server stays on loopback and a proxy
// terminates TLS, forwarding the public Host and `X-Forwarded-Proto: https`.
test("the packaged server works behind a TLS-terminating reverse proxy", { timeout: 60_000 }, async (t) => {
  const fixture = mkdtempSync(path.join(tmpdir(), "openaide-web-proxied-"));
  t.after(() => rmSync(fixture, { recursive: true, force: true }));
  const staticRoot = path.join(fixture, "static");
  const appServerPath = path.join(fixture, "app-server.mjs");
  mkdirSync(staticRoot);
  writeFileSync(path.join(staticRoot, "index.html"), "<html><head></head><body><div id=\"root\"></div></body></html>");
  writeFileSync(appServerPath, fakeAppServerSource());
  chmodSync(appServerPath, 0o755);

  const baseEnv = Object.fromEntries(Object.entries(process.env).filter(([name]) => !name.startsWith("OPENAIDE_")));
  const webServer = spawn(process.execPath, ["src/server.mjs"], {
    cwd: new URL("..", import.meta.url),
    env: {
      ...baseEnv,
      OPENAIDE_APP_SERVER_PATH: appServerPath,
      OPENAIDE_WEB_ALLOWED_HOSTS: PUBLIC_HOST,
      OPENAIDE_WEB_HOST: "127.0.0.1",
      OPENAIDE_WEB_PORT: "0",
      OPENAIDE_WEB_STATIC_ROOT: staticRoot,
      XDG_DATA_HOME: path.join(fixture, "data"),
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  t.after(() => {
    if (webServer.exitCode !== null) return undefined;
    return new Promise((resolve) => {
      webServer.once("exit", resolve);
      webServer.kill("SIGTERM");
    });
  });
  const webPort = await listeningPort(webServer);
  const proxy = await startTlsProxy(t, fixture, webPort);

  const page = await proxy.request({ path: "/" });
  assert.equal(page.status, 200);
  assert.match(page.body, /data-shell="web"/);

  const rpc = (origin) => proxy.request({
    method: "POST",
    path: "/__openaide-app-server/probe",
    headers: { "content-type": "application/json", origin },
    body: JSON.stringify({ jsonrpc: "2.0", id: "probe", method: "client/probe", params: {} }),
  });
  // The HTTP transport: the browser's HTTPS origin is the one the server expects.
  const accepted = await rpc(`https://${PUBLIC_HOST}`);
  assert.equal(accepted.status, 200);
  assert.equal(JSON.parse(accepted.body).id, "probe");
  const downgraded = await rpc(`http://${PUBLIC_HOST}`);
  assert.equal(downgraded.status, 403);
  assert.equal(downgraded.body, "Origin not allowed");
  const foreign = await rpc("https://elsewhere.test");
  assert.equal(foreign.status, 403);

  // The WebSocket transport: the upgrade crosses both proxies with the shell's credential.
  const upgrade = await proxy.upgrade("/__openaide-app-server/probe?connectionId=proxied", `https://${PUBLIC_HOST}`);
  assert.match(upgrade, /^HTTP\/1\.1 101 /);
  assert.match(upgrade, /app-server-socket:\/rpc\?connectionId=proxied/);

  const unlistedHost = await proxy.request({ path: "/", host: "unlisted.test" });
  assert.equal(unlistedHost.status, 403);
  assert.equal(unlistedHost.body, "Host not allowed");
});

async function startTlsProxy(t, fixture, upstreamPort) {
  const keyPath = path.join(fixture, "proxy.key");
  const certPath = path.join(fixture, "proxy.crt");
  execFileSync("openssl", [
    "req", "-x509", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:prime256v1", "-nodes",
    "-keyout", keyPath, "-out", certPath, "-days", "1",
    "-subj", `/CN=${PUBLIC_HOST}`, "-addext", `subjectAltName=DNS:${PUBLIC_HOST},DNS:unlisted.test`,
  ], { stdio: "ignore" });
  const cert = readFileSync(certPath);
  const forwarded = (req) => ({ ...req.headers, "x-forwarded-proto": "https" });
  const server = https.createServer({ key: readFileSync(keyPath), cert }, (req, res) => {
    const upstream = http.request({ agent: false, port: upstreamPort, path: req.url, method: req.method, headers: forwarded(req) }, (response) => {
      res.writeHead(response.statusCode, response.headers);
      response.pipe(res);
    });
    upstream.once("error", () => res.destroy());
    req.pipe(upstream);
  });
  server.on("upgrade", (req, socket, head) => {
    const upstream = http.request({ agent: false, port: upstreamPort, path: req.url, method: req.method, headers: forwarded(req) });
    upstream.once("upgrade", (response, upstreamSocket, upstreamHead) => {
      socket.write(`HTTP/1.1 ${response.statusCode} ${response.statusMessage}\r\n`);
      for (let index = 0; index < response.rawHeaders.length; index += 2) {
        socket.write(`${response.rawHeaders[index]}: ${response.rawHeaders[index + 1]}\r\n`);
      }
      socket.write("\r\n");
      if (upstreamHead.length) socket.write(upstreamHead);
      if (head.length) upstreamSocket.write(head);
      upstreamSocket.pipe(socket);
      socket.pipe(upstreamSocket);
    });
    upstream.once("response", () => socket.destroy());
    upstream.once("error", () => socket.destroy());
    upstream.end();
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  t.after(() => {
    server.closeAllConnections();
    server.close();
  });
  const connection = (host) => ({
    host: "127.0.0.1",
    port: server.address().port,
    servername: host,
    ca: cert,
    agent: false,
  });

  return {
    request({ body, headers = {}, host = PUBLIC_HOST, method = "GET", path: requestPath }) {
      return new Promise((resolve, reject) => {
        const request = https.request({ ...connection(host), method, path: requestPath, headers: { ...headers, host } }, (response) => {
          const chunks = [];
          response.on("data", (chunk) => chunks.push(chunk));
          response.on("end", () => resolve({ status: response.statusCode, body: Buffer.concat(chunks).toString("utf8") }));
        });
        request.once("error", reject);
        request.end(body);
      });
    },
    upgrade(requestPath, origin) {
      return new Promise((resolve, reject) => {
        const request = https.request({
          ...connection(PUBLIC_HOST),
          path: requestPath,
          headers: {
            connection: "Upgrade",
            host: PUBLIC_HOST,
            origin,
            upgrade: "websocket",
            "sec-websocket-key": "dGhlIHNhbXBsZSBub25jZQ==",
            "sec-websocket-version": "13",
          },
        });
        request.once("upgrade", (response, socket, head) => {
          let received = `HTTP/1.1 ${response.statusCode} ${response.statusMessage}\r\n${head.toString("utf8")}`;
          socket.setEncoding("utf8");
          socket.on("data", (chunk) => { received += chunk; });
          socket.once("end", () => resolve(received));
          socket.once("error", reject);
        });
        request.once("response", (response) => reject(new Error(`Upgrade was answered with HTTP ${response.statusCode}`)));
        request.once("error", reject);
        request.end();
      });
    },
  };
}

// Answers RPC over HTTP and accepts an upgrade only with the shell's credential,
// echoing the path it was reached on so the test can see the whole route.
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
server.on("upgrade", (request, socket) => {
  if (request.headers.authorization !== \`Bearer \${authToken}\`) {
    socket.end("HTTP/1.1 401 Unauthorized\\r\\nContent-Length: 0\\r\\n\\r\\n");
    return;
  }
  socket.end([
    "HTTP/1.1 101 Switching Protocols",
    "Connection: Upgrade",
    "Upgrade: websocket",
    "",
    \`app-server-socket:\${request.url}\`,
  ].join("\\r\\n"));
});
server.listen(0, "127.0.0.1", () => console.log(JSON.stringify({
  kind: "localHttp",
  endpointUrl: \`http://127.0.0.1:\${server.address().port}/rpc\`,
  authToken,
})));
process.once("SIGTERM", () => server.close(() => process.exit(0)));
`;
}
