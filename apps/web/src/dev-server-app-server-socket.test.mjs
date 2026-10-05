import assert from "node:assert/strict";
import { once } from "node:events";
import http from "node:http";
import test from "node:test";
import { createAppServerSocketProxy } from "./dev-server-app-server-socket.mjs";

test("relays an authenticated App Server socket in both directions", async (t) => {
  const upstreamRequests = [];
  const upstream = http.createServer();
  upstream.on("upgrade", (request, socket) => {
    upstreamRequests.push(request);
    socket.write("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\r\n");
    socket.on("data", (chunk) => socket.write(`echo:${chunk}`));
    socket.on("end", () => socket.end());
  });
  const { port, events } = await startProxy(t, upstream);

  const browser = await upgrade(port, "/__openaide-app-server/probe?connectionId=client-1", {
    authorization: "Basic browser-credential",
    cookie: "session=browser-cookie",
  });
  assert.equal(browser.response.statusCode, 101);
  browser.socket.write("hello");
  const [reply] = await once(browser.socket, "data");

  assert.equal(String(reply), "echo:hello");
  assert.equal(upstreamRequests[0].url, "/rpc?connectionId=client-1");
  assert.equal(upstreamRequests[0].headers.authorization, "Bearer app-server-token");
  assert.equal(upstreamRequests[0].headers.cookie, undefined);
  assert.equal(upstreamRequests[0].headers["sec-websocket-key"], "dGhlIHNhbXBsZSBub25jZQ==");

  browser.socket.destroy();
  await waitFor(() => events.some(({ event }) => event === "web_proxy_socket_closed"));
  assert.deepEqual(events.map(({ event }) => event), [
    "web_proxy_socket_started",
    "web_proxy_socket_connected",
    "web_proxy_socket_closed",
  ]);
  const closed = events.at(-1).fields;
  assert.equal(closed.reason_code, "browser_closed");
  assert.equal(closed.connection_id, "client-1");
  // Both counters include the handshake, so only their presence is asserted.
  assert.ok(closed.bytes_from_browser > 0 && closed.bytes_from_app_server > 0);
  assert.doesNotMatch(JSON.stringify(events), /app-server-token|browser-credential/);
});

test("reports the App Server closing a relayed socket", async (t) => {
  const upstream = http.createServer();
  upstream.on("upgrade", (_request, socket) => {
    socket.end("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\r\n");
  });
  const { port, events } = await startProxy(t, upstream);

  const browser = await upgrade(port, "/__openaide-app-server/probe");
  browser.socket.resume();
  await once(browser.socket, "close");

  await waitFor(() => events.some(({ event }) => event === "web_proxy_socket_closed"));
  assert.equal(events.at(-1).fields.reason_code, "app_server_closed");
});

test("passes on a handshake the App Server declines", async (t) => {
  const upstream = http.createServer((_request, response) => {
    response.writeHead(400).end();
  });
  // Without an upgrade listener Node answers the upgrade as a plain request.
  const { port, events } = await startProxy(t, upstream);

  const status = await upgradeStatus(port, "/__openaide-app-server/probe");

  assert.equal(status, 400);
  assert.equal(events.at(-1).event, "web_proxy_socket_failed");
  assert.equal(events.at(-1).fields.reason_code, "app_server_rejected");
});

test("answers 503 when the App Server cannot start", async (t) => {
  const { port, events } = await startProxy(t, undefined, async () => {
    throw new Error("spawn failed at /private/path");
  });

  const status = await upgradeStatus(port, "/__openaide-app-server/probe");

  assert.equal(status, 503);
  assert.equal(events.at(-1).fields.reason_code, "app_server_unavailable");
  assert.doesNotMatch(JSON.stringify(events), /private/);
});

async function startProxy(t, upstream, startAppServer = async () => {}) {
  const upstreamPort = upstream ? await listen(upstream) : 1;
  const events = [];
  const record = (event, fields) => events.push({ event, fields });
  const proxy = createAppServerSocketProxy({
    startAppServer,
    currentEndpoint: () => ({
      url: new URL(`http://127.0.0.1:${upstreamPort}/rpc`),
      authToken: "app-server-token",
    }),
    logger: { info: record, warn: record },
  });
  const server = http.createServer();
  const sockets = new Set();
  server.on("upgrade", (request, socket, head) => {
    sockets.add(socket);
    void proxy(request, socket, head, new URL(request.url, "http://localhost"));
  });
  const port = await listen(server);
  t.after(async () => {
    for (const socket of sockets) socket.destroy();
    upstream?.closeAllConnections();
    await Promise.all([close(server), upstream ? close(upstream) : undefined]);
  });
  return { port, events };
}

function upgradeRequest(port, path, headers = {}) {
  const request = http.request({
    hostname: "127.0.0.1",
    port,
    path,
    headers: {
      connection: "Upgrade",
      upgrade: "websocket",
      "sec-websocket-key": "dGhlIHNhbXBsZSBub25jZQ==",
      "sec-websocket-version": "13",
      ...headers,
    },
  });
  request.end();
  return request;
}

async function upgrade(port, path, headers) {
  const [response, socket] = await once(upgradeRequest(port, path, headers), "upgrade");
  return { response, socket };
}

async function upgradeStatus(port, path) {
  const [response] = await once(upgradeRequest(port, path), "response");
  response.resume();
  return response.statusCode;
}

async function waitFor(condition) {
  const deadline = Date.now() + 2_000;
  while (!condition()) {
    if (Date.now() > deadline) throw new Error("condition was not met");
    await new Promise((resolve) => setTimeout(resolve, 5));
  }
}

function listen(server) {
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => resolve(server.address().port));
  });
}

function close(server) {
  if (!server.listening) return Promise.resolve();
  return new Promise((resolve, reject) => {
    server.close((error) => error ? reject(error) : resolve());
  });
}
