import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import test from "node:test";
import { createAppServerManager } from "./app-server-manager.mjs";

test("concurrent App Server starts share one spawned runtime", async () => {
  const children = [];
  const handoff = deferred();
  const requests = [];
  const manager = createAppServerManager({
    connectionUrl: (connection) => new URL(connection.endpointUrl),
    readHandoffConnection: () => handoff.promise,
    requestAppServer: async (connection, connectionId, body) => {
      requests.push({ connection, connectionId, body });
      return {};
    },
    spawnAppServer: () => {
      const child = childProcess();
      children.push(child);
      return child;
    },
  });

  const first = manager.startAppServer();
  const second = manager.startAppServer();
  handoff.resolve({
    authToken: "token",
    endpointUrl: "http://127.0.0.1:1234/probe",
  });

  const [firstResult, secondResult] = await Promise.all([first, second]);

  assert.equal(children.length, 1);
  assert.equal(firstResult.appServer, children[0]);
  assert.equal(secondResult.appServer, children[0]);
  assert.equal(manager.currentConnection().authToken, "token");
  assert.equal(manager.currentUrl().href, "http://127.0.0.1:1234/probe");
  assert.equal(requests.length, 1);
  assert.equal(requests[0].connectionId, "web-app-shell");
  assert.equal(requests[0].body.method, "client/initialize");
  assert.deepEqual(requests[0].body.params, {
    clientInstanceId: "web-app-shell",
    shell: { kind: "web" },
    requestedSurface: { kind: "home" },
    capabilities: {
      protocol: ["requestResponses", "stableClientRequestIds", "resync"],
      shell: [],
    },
  });
});

test("failed App Server handoff clears state so a later start can retry", async () => {
  const handoffs = [deferred(), deferred()];
  const children = [];
  const manager = createAppServerManager({
    connectionUrl: (connection) => new URL(connection.endpointUrl),
    readHandoffConnection: () => handoffs[children.length - 1].promise,
    requestAppServer: async () => ({}),
    spawnAppServer: () => {
      const child = childProcess();
      children.push(child);
      return child;
    },
  });

  const first = manager.startAppServer();
  handoffs[0].reject(new Error("handoff failed"));
  await assert.rejects(first, /handoff failed/);

  assert.equal(children[0].killed, true);
  assert.equal(manager.currentConnection(), undefined);

  const second = manager.startAppServer();
  handoffs[1].resolve({
    authToken: "token-2",
    endpointUrl: "http://127.0.0.1:4321/probe",
  });
  await second;

  assert.equal(children.length, 2);
  assert.equal(manager.currentConnection().authToken, "token-2");
});

test("attached handoff child exit keeps the live App Server connection", async () => {
  const children = [];
  const requests = [];
  const manager = createAppServerManager({
    connectionUrl: (connection) => new URL(connection.endpointUrl),
    readHandoffConnection: async () => ({
      authToken: `token-${children.length}`,
      endpointUrl: "http://127.0.0.1:1234/probe",
    }),
    requestAppServer: async (connection, connectionId, body) => {
      requests.push({ connection, connectionId, body });
      return {};
    },
    spawnAppServer: () => {
      const child = childProcess();
      children.push(child);
      return child;
    },
  });

  await manager.startAppServer();
  children[0].emit("exit");
  await manager.startAppServer();

  assert.equal(children.length, 1);
  assert.equal(manager.currentConnection().authToken, "token-1");
  assert.equal(manager.currentUrl().href, "http://127.0.0.1:1234/probe");
  assert.equal(requests.filter((request) => request.body.method === "client/initialize").length, 1);
});

test("clearing an App Server connection lets a later start hand off again", async () => {
  const children = [];
  const manager = createAppServerManager({
    connectionUrl: (connection) => new URL(connection.endpointUrl),
    readHandoffConnection: async () => ({
      authToken: `token-${children.length}`,
      endpointUrl: `http://127.0.0.1:${1000 + children.length}/probe`,
    }),
    requestAppServer: async () => ({}),
    spawnAppServer: () => {
      const child = childProcess();
      children.push(child);
      return child;
    },
  });

  await manager.startAppServer();
  manager.clearConnection();
  await manager.startAppServer();

  assert.equal(children.length, 2);
  assert.equal(manager.currentConnection().authToken, "token-2");
  assert.equal(manager.currentUrl().href, "http://127.0.0.1:1002/probe");
});

test("web shell heartbeat keeps App Server alive while the web process runs", async () => {
  const requests = [];
  const manager = createAppServerManager({
    connectionUrl: (connection) => new URL(connection.endpointUrl),
    readHandoffConnection: async () => ({
      authToken: "token",
      endpointUrl: "http://127.0.0.1:1234/probe",
    }),
    requestAppServer: async (connection, connectionId, body) => {
      requests.push({ connection, connectionId, body });
      return {};
    },
    shellHeartbeatIntervalMs: 5,
    spawnAppServer: childProcess,
  });

  await manager.startAppServer();
  await waitFor(() => requests.some((request) => request.body.method === "client/heartbeat"));

  const heartbeat = requests.find((request) => request.body.method === "client/heartbeat");
  assert.equal(heartbeat.connectionId, "web-app-shell");
});

test("heartbeat diagnostics report one failure transition and one recovery", async () => {
  const events = [];
  let heartbeatAttempts = 0;
  const manager = createAppServerManager({
    connectionUrl: (connection) => new URL(connection.endpointUrl),
    logger: recordingLogger(events),
    readHandoffConnection: async () => ({
      authToken: "token",
      endpointUrl: "http://127.0.0.1:1234/probe",
    }),
    requestAppServer: async (_connection, _connectionId, body) => {
      if (body.method !== "client/heartbeat") return {};
      heartbeatAttempts += 1;
      if (heartbeatAttempts < 3) throw new TypeError("private transport detail");
      return {};
    },
    shellHeartbeatIntervalMs: 5,
    spawnAppServer: childProcess,
  });

  await manager.startAppServer();
  await waitFor(() => events.some(({ event }) => event === "app_server_heartbeat_recovered"));
  manager.clearConnection();

  assert.deepEqual(
    events.filter(({ event }) => event.startsWith("app_server_heartbeat_")),
    [
      { level: "warn", event: "app_server_heartbeat_failed", fields: { error_kind: "TypeError" } },
      { level: "info", event: "app_server_heartbeat_recovered", fields: {} },
    ],
  );
  assert.doesNotMatch(JSON.stringify(events), /private transport detail/);
});

function childProcess() {
  const child = new EventEmitter();
  child.killed = false;
  child.kill = () => {
    child.killed = true;
    child.emit("exit");
  };
  return child;
}

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, reject, resolve };
}

function recordingLogger(events) {
  return Object.fromEntries(["info", "warn", "error"].map((level) => [
    level,
    (event, fields = {}) => events.push({ level, event, fields }),
  ]));
}

function waitFor(predicate) {
  return new Promise((resolve, reject) => {
    const started = Date.now();
    const timer = setInterval(() => {
      if (predicate()) {
        clearInterval(timer);
        resolve();
        return;
      }
      if (Date.now() - started > 200) {
        clearInterval(timer);
        reject(new Error("timed out waiting for condition"));
      }
    }, 5);
  });
}
