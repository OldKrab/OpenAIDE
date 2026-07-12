import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import test from "node:test";
import { pipeProxyResponse, watchPendingProxyResponse } from "./dev-server-streams.mjs";

test("pipeProxyResponse treats destination ECONNRESET as closed client instead of crashing", async () => {
  const proxyRes = new FakeReadable();
  const res = new EventEmitter();
  res.write = () => true;
  res.end = () => {};

  const done = pipeProxyResponse(proxyRes, res);
  res.emit("error", Object.assign(new Error("reset"), { code: "ECONNRESET" }));

  await assert.doesNotReject(done);
});

test("pipeProxyResponse cancels the upstream stream when the browser disconnects", async () => {
  const proxyRes = new FakeReadable();
  const res = new EventEmitter();
  res.write = () => true;
  res.end = () => {};

  const done = pipeProxyResponse(proxyRes, res);
  res.emit("close");

  await done;
  assert.equal(proxyRes.destroyed, true);
});

test("watchPendingProxyResponse cancels an upstream request before response headers", async () => {
  const proxyReq = new FakeReadable();
  const res = new EventEmitter();
  const pending = watchPendingProxyResponse(proxyReq, res);

  res.emit("close");

  await pending.cancelled;
  assert.equal(proxyReq.destroyed, true);
  assert.equal(pending.handoff(), false);
});

test("pipeProxyResponse rejects non-reset stream errors", async () => {
  const proxyRes = new FakeReadable();
  const res = new EventEmitter();
  res.write = () => true;
  res.end = () => {};

  const done = pipeProxyResponse(proxyRes, res);
  proxyRes.emit("error", Object.assign(new Error("boom"), { code: "EOTHER" }));

  await assert.rejects(done, /boom/);
});

class FakeReadable extends EventEmitter {
  destroyed = false;

  pipe(destination) {
    this.destination = destination;
    return destination;
  }

  destroy() {
    this.destroyed = true;
  }
}
