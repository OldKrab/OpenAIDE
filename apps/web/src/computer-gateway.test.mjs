import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { PassThrough } from "node:stream";
import test from "node:test";
import { ComputerRefusal, createComputerGateway } from "./computer-gateway.mjs";

const COMPUTER = "a".repeat(64);

/** A gateway process that answers by the script it is given, and records what it read. */
function fakeProcess(answer) {
  const child = new EventEmitter();
  child.pid = 4242;
  child.stdin = new PassThrough();
  child.stdout = new PassThrough();
  child.requests = [];
  child.killed = false;
  child.kill = (signal) => {
    child.killed = true;
    queueMicrotask(() => child.emit("exit", null, signal ?? "SIGTERM"));
  };
  child.send = (message) => child.stdout.write(`${JSON.stringify(message)}\n`);
  let buffered = "";
  child.stdin.on("data", (chunk) => {
    buffered += chunk;
    for (let newline = buffered.indexOf("\n"); newline >= 0; newline = buffered.indexOf("\n")) {
      const request = JSON.parse(buffered.slice(0, newline));
      buffered = buffered.slice(newline + 1);
      child.requests.push(request);
      answer?.(request, child);
    }
  });
  queueMicrotask(() => child.send({ event: "ready" }));
  return child;
}

function recordingLogger() {
  const events = [];
  const record = (level) => (event, fields = {}) => events.push({ level, event, fields });
  return { events, logger: { info: record("info"), warn: record("warn"), error: record("error") } };
}

function answering(results) {
  return (request, child) => {
    const answer = results[request.method];
    if (typeof answer === "function") answer(request, child);
    else if (answer?.error) child.send({ id: request.id, error: answer.error });
    else child.send({ id: request.id, result: answer ?? {} });
  };
}

test("the process starts on first use and answers each call by its id", async () => {
  const spawned = [];
  const { events, logger } = recordingLogger();
  const gateway = createComputerGateway({
    logger,
    spawnGateway: () => {
      const child = fakeProcess(answering({
        computers: { computers: [{ id: COMPUTER, name: "Desk" }] },
        connect: { kind: "localHttp", endpointUrl: "http://127.0.0.1:4100/probe", authToken: "gateway-token" },
      }));
      spawned.push(child);
      return child;
    },
  });
  assert.equal(spawned.length, 0, "nothing starts before a browser needs another computer");

  assert.deepEqual(await gateway.computers(), [{ id: COMPUTER, name: "Desk" }]);
  const [first, second] = await Promise.all([gateway.connect(COMPUTER), gateway.connect(COMPUTER)]);
  assert.equal(first, second, "concurrent requests share one connection");
  assert.equal(first.url.href, "http://127.0.0.1:4100/probe");
  assert.equal(first.authToken, "gateway-token");
  // Cached: the list and the endpoint are asked once.
  await gateway.computers();
  await gateway.connect(COMPUTER);
  assert.equal(spawned.length, 1);
  assert.deepEqual(spawned[0].requests, [
    { id: 1, method: "computers" },
    { id: 2, method: "connect", computer: COMPUTER },
  ]);

  const names = events.map((entry) => entry.event);
  assert.deepEqual(names.slice(0, 3), [
    "computer_gateway_call_started",
    "computer_gateway_process_started",
    "computer_gateway_process_ready",
  ]);
  const completed = events.filter((entry) => entry.event === "computer_gateway_call_completed");
  assert.deepEqual(completed.map((entry) => [entry.fields.method, entry.fields.outcome]), [["computers", "ok"], ["connect", "ok"]]);
  // Metadata only: no id, name, endpoint, or token reaches the log.
  const logged = JSON.stringify(events);
  for (const secret of [COMPUTER, "Desk", "gateway-token", "4100"]) assert.equal(logged.includes(secret), false, secret);
});

test("a refusal carries the gateway's reason and sentence, and is asked again next time", async () => {
  let refuse = true;
  const { events, logger } = recordingLogger();
  const gateway = createComputerGateway({
    logger,
    spawnGateway: () => fakeProcess(answering({
      connect: (request, child) => child.send(refuse
        ? { id: request.id, error: { reason: "unreachable", message: "Desk is unreachable." } }
        : { id: request.id, result: { kind: "localHttp", endpointUrl: "http://127.0.0.1:4101/probe", authToken: "t" } }),
    })),
  });

  await assert.rejects(gateway.connect(COMPUTER), (error) => {
    assert.ok(error instanceof ComputerRefusal);
    assert.equal(error.reason, "unreachable");
    assert.equal(error.message, "Desk is unreachable.");
    return true;
  });
  refuse = false;
  assert.equal((await gateway.connect(COMPUTER)).url.port, "4101");
  const refused = events.find((entry) => entry.fields.outcome === "refused");
  assert.equal(refused.fields.reason, "unreachable");
  assert.equal(JSON.stringify(events).includes("Desk"), false, "the sentence stays out of the log");
});

test("a computer that refused this device is asked for again", async () => {
  let port = 4200;
  let child;
  const { events, logger } = recordingLogger();
  const gateway = createComputerGateway({
    logger,
    spawnGateway: () => (child = fakeProcess(answering({
      connect: (request, process) => process.send({
        id: request.id,
        result: { kind: "localHttp", endpointUrl: `http://127.0.0.1:${++port}/probe`, authToken: "t" },
      }),
    }))),
  });
  assert.equal((await gateway.connect(COMPUTER)).url.port, "4201");
  child.send({ event: "failure", computer: COMPUTER, reason: "not_trusted" });
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal((await gateway.connect(COMPUTER)).url.port, "4202");
  assert.ok(events.some((entry) => entry.event === "computer_gateway_computer_refused" && entry.fields.reason === "not_trusted"));
});

test("an exited process fails its calls, is logged, and starts again on the next use", async () => {
  const spawned = [];
  const { events, logger } = recordingLogger();
  const gateway = createComputerGateway({
    logger,
    spawnGateway: () => {
      const child = fakeProcess((request, process) => {
        if (spawned.length === 1) process.emit("exit", 1, null);
        else process.send({ id: request.id, result: { computers: [] } });
      });
      spawned.push(child);
      return child;
    },
  });

  await assert.rejects(gateway.computers(), (error) => error.reason === "gateway_unavailable");
  const exited = events.find((entry) => entry.event === "computer_gateway_process_exited");
  assert.equal(exited.level, "warn");
  assert.equal(exited.fields.expected, false);
  assert.equal(exited.fields.exit_code, 1);

  assert.deepEqual(await gateway.computers(), []);
  assert.equal(spawned.length, 2);
});

test("a process that never answers is bounded", async () => {
  const gateway = createComputerGateway({
    logger: recordingLogger().logger,
    // timing: expiry — the process never answers, so the bound is the only outcome.
    callTimeoutMs: 20,
    spawnGateway: () => fakeProcess(() => {}),
  });
  await assert.rejects(gateway.connect(COMPUTER), (error) => error.reason === "timeout");
});

test("pairing and forgetting drop what was known about the computer", async () => {
  let listed = [];
  let port = 4300;
  const gateway = createComputerGateway({
    logger: recordingLogger().logger,
    spawnGateway: () => fakeProcess(answering({
      computers: (request, child) => child.send({ id: request.id, result: { computers: listed } }),
      pair: (request, child) => {
        listed = [{ id: COMPUTER, name: "Desk" }];
        child.send({ id: request.id, result: { computer: listed[0] } });
      },
      connect: (request, child) => child.send({
        id: request.id,
        result: { kind: "localHttp", endpointUrl: `http://127.0.0.1:${++port}/probe`, authToken: "t" },
      }),
      forget: (request, child) => {
        listed = [];
        child.send({ id: request.id, result: {} });
      },
    })),
  });

  assert.deepEqual(await gateway.computers(), []);
  assert.deepEqual(await gateway.pair("OAI1CODE", { key: "k", name: "Here" }), { id: COMPUTER, name: "Desk" });
  assert.deepEqual(await gateway.computers(), [{ id: COMPUTER, name: "Desk" }]);
  assert.equal((await gateway.connect(COMPUTER)).url.port, "4301");
  await gateway.pair("OAI1CODE");
  assert.equal((await gateway.connect(COMPUTER)).url.port, "4302", "pairing again asks for a new connection");
  await gateway.forget(COMPUTER);
  assert.deepEqual(await gateway.computers(), []);
});

test("a connection handed out while the computer is forgotten is not kept", async () => {
  let port = 4400;
  let forgetReceived;
  const received = new Promise((resolve) => { forgetReceived = resolve; });
  let answerForget;
  const gateway = createComputerGateway({
    logger: recordingLogger().logger,
    spawnGateway: () => fakeProcess(answering({
      connect: (request, child) => child.send({
        id: request.id,
        result: { kind: "localHttp", endpointUrl: `http://127.0.0.1:${++port}/probe`, authToken: "t" },
      }),
      // The gateway answers a connect that came before the forget took the computer's slot.
      forget: (request, child) => {
        answerForget = () => child.send({ id: request.id, result: {} });
        forgetReceived();
      },
    })),
  });

  const forgetting = gateway.forget(COMPUTER);
  await received;
  assert.equal((await gateway.connect(COMPUTER)).url.port, "4401");
  answerForget();
  await forgetting;
  assert.equal((await gateway.connect(COMPUTER)).url.port, "4402", "the gateway is asked again");
});

test("stopping as the server leaves is logged as expected", async () => {
  let child;
  const { events, logger } = recordingLogger();
  const gateway = createComputerGateway({
    logger,
    spawnGateway: () => (child = fakeProcess(answering({ computers: { computers: [] } }))),
  });
  await gateway.computers();
  gateway.stop("SIGTERM");
  await new Promise((resolve) => child.once("exit", resolve));
  assert.equal(child.killed, true);
  const exited = events.find((entry) => entry.event === "computer_gateway_process_exited");
  assert.equal(exited.level, "info");
  assert.equal(exited.fields.expected, true);
  assert.equal(gateway.currentProcess(), undefined);
});
