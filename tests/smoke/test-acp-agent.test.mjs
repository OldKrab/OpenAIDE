import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import path from "node:path";
import readline from "node:readline";
import test from "node:test";
import { fileURLToPath } from "node:url";

const fixture = path.join(path.dirname(fileURLToPath(import.meta.url)), "fixtures/test-acp-agent.mjs");

test("streams updates after a permission request and completes after the client responds", async (t) => {
  const agent = connectAgent(t);
  await agent.request("initialize", { protocolVersion: 1 });
  const created = await agent.request("session/new", { cwd: process.cwd(), mcpServers: [] });
  const prompt = agent.request("session/prompt", {
    sessionId: created.sessionId,
    prompt: [{ type: "text", text: "smoke:permission" }],
  });

  const permission = await agent.next((message) => message.method === "session/request_permission");
  await agent.next((message) => message.params?.update?.content?.text === "Permission is still pending");
  agent.respond(permission.id, { outcome: { outcome: "selected", optionId: "allow-once" } });

  assert.equal((await prompt).stopReason, "end_turn");
  await agent.next((message) => message.params?.update?.content?.text === "Permission result: allow-once");
});

test("accepts steering while the primary prompt remains active and then cancels it", async (t) => {
  const agent = connectAgent(t);
  await agent.request("initialize", { protocolVersion: 1 });
  const { sessionId } = await agent.request("session/new", { cwd: process.cwd(), mcpServers: [] });
  const held = agent.request("session/prompt", {
    sessionId,
    prompt: [{ type: "text", text: "smoke:hold" }],
  });
  await agent.next((message) => message.params?.update?.content?.text === "Waiting for steering");

  const steered = await agent.request("session/prompt", {
    sessionId,
    prompt: [{ type: "text", text: "follow up" }],
  });
  assert.equal(steered.stopReason, "end_turn");
  agent.notify("session/cancel", { sessionId });
  assert.equal((await held).stopReason, "cancelled");
});

test("requests structured elicitation and continues with the submitted value", async (t) => {
  const agent = connectAgent(t);
  await agent.request("initialize", { protocolVersion: 1 });
  const { sessionId } = await agent.request("session/new", { cwd: process.cwd(), mcpServers: [] });
  const prompt = agent.request("session/prompt", {
    sessionId,
    prompt: [{ type: "text", text: "smoke:question" }],
  });
  const question = await agent.next((message) => message.method === "elicitation/create");
  assert.equal(question.params.requestedSchema.properties.projectName.type, "string");
  agent.respond(question.id, { action: "accept", content: { projectName: "Alpha" } });
  assert.equal((await prompt).stopReason, "end_turn");
  await agent.next((message) => message.params?.update?.content?.text === "Question result: Alpha");
});

function connectAgent(t) {
  const child = spawn(process.execPath, [fixture], { stdio: ["pipe", "pipe", "pipe"] });
  const lines = readline.createInterface({ input: child.stdout });
  const pending = new Map();
  const inbox = [];
  const waiters = [];
  let nextId = 1;

  lines.on("line", (line) => {
    const message = JSON.parse(line);
    const request = pending.get(String(message.id));
    if (!message.method && request) {
      pending.delete(String(message.id));
      if (message.error) request.reject(new Error(message.error.message));
      else request.resolve(message.result);
      return;
    }
    const waiter = waiters.find(({ predicate }) => predicate(message));
    if (waiter) {
      waiters.splice(waiters.indexOf(waiter), 1);
      waiter.resolve(message);
    } else {
      inbox.push(message);
    }
  });

  t.after(async () => {
    child.kill("SIGTERM");
    if (child.exitCode === null) await once(child, "exit");
  });

  return {
    request(method, params) {
      const id = `contract-${nextId++}`;
      child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
      return new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
    },
    notify(method, params) {
      child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method, params })}\n`);
    },
    respond(id, result) {
      child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, result })}\n`);
    },
    next(predicate) {
      const index = inbox.findIndex(predicate);
      if (index >= 0) return Promise.resolve(inbox.splice(index, 1)[0]);
      return new Promise((resolve) => waiters.push({ predicate, resolve }));
    },
  };
}
