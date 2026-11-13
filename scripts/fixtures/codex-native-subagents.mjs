#!/usr/bin/env node
// A native Codex app-server stand-in for `scripts/smoke-codex-subagents.mjs`. It
// answers the requests the adapter sends while loading and prompting one session,
// and every turn spawns one child agent, so the smoke exercises the adapter's real
// Codex-to-ACP native and legacy Subagent paths without a model or credentials.
import { appendFileSync, readFileSync } from "node:fs";
import { createInterface } from "node:readline";

const fixture = JSON.parse(readFileSync(process.env.OPENAIDE_CODEX_SUBAGENTS_FIXTURE, "utf8"));
const thread = {
  id: "native-session", sessionId: "native-session", preview: "Fixture session",
  createdAt: 1, updatedAt: 1, cwd: fixture.cwd, modelProvider: "openai",
  path: fixture.historyPath, turns: [], historyMode: "legacy", status: { type: "idle" }, source: "appServer",
};
const childThread = { ...thread, id: "child-fixture", sessionId: "child-fixture", path: null, turns: [] };
const send = (message) => process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", ...message })}\n`);
const model = (id, isDefault) => ({
  id, model: id, displayName: id, description: id, isDefault,
  supportedReasoningEfforts: [{ reasoningEffort: "medium", description: "Medium" }],
  defaultReasoningEffort: "medium", inputModalities: ["text"], supportsPersonality: false,
});
const policy = { approvalPolicy: "never", approvalsReviewer: "user", sandbox: { type: "dangerFullAccess" } };
let turnNumber = 0;
for await (const line of createInterface({ input: process.stdin })) {
  const request = JSON.parse(line);
  appendFileSync(fixture.calls, `${JSON.stringify(request)}\n`);
  if (request.id === undefined) continue;
  let result;
  switch (request.method) {
    case "initialize": result = { userAgent: "fixture", codexHome: fixture.cwd }; break;
    case "account/read": result = { account: { type: "apiKey" }, requiresOpenaiAuth: false }; break;
    case "config/read": result = { config: { model_provider: "openai" }, origins: {}, layers: [] }; break;
    case "skills/list": result = { data: [] }; break;
    case "skills/extraRoots/set": result = {}; break;
    case "permissionProfile/list": result = { data: [], nextCursor: null }; break;
    case "model/list": result = { data: [model("native-luna", true)], nextCursor: null }; break;
    case "thread/resume": result = {
      thread, model: "native-luna", modelProvider: "openai", reasoningEffort: "medium",
      serviceTier: null, cwd: fixture.cwd, ...policy,
    }; break;
    case "thread/read": result = { thread: request.params.threadId === childThread.id ? childThread : thread }; break;
    case "thread/items/list": {
      const selected = request.params.threadId === childThread.id ? childThread : thread;
      let data = selected.turns.flatMap((turn) => turn.items.map((item) => ({ turnId: turn.id, item })));
      if (request.params.sortDirection === "desc") data.reverse();
      result = { data: data.slice(0, request.params.limit ?? data.length), nextCursor: null };
      break;
    }
    case "thread/goal/get": result = { goal: null }; break;
    case "thread/unsubscribe": result = {}; break;
    case "turn/start": {
      const turn = { id: `turn-fixture-${++turnNumber}`, items: [], status: "inProgress", error: null };
      result = { turn };
      setImmediate(() => emitSubagentTurn(turn));
      break;
    }
    default:
      send({ id: request.id, error: { code: -32601, message: `Unsupported fixture method: ${request.method}` } });
      continue;
  }
  send({ id: request.id, result });
}

/** Drives the maintained adapter's real Codex-to-ACP native and legacy paths. */
function emitSubagentTurn(turn) {
  const spawn = {
    type: "collabAgentToolCall", id: "spawn-fixture", tool: "spawnAgent", status: "completed",
    senderThreadId: thread.id, receiverThreadIds: [childThread.id],
    agentsStates: { [childThread.id]: { status: "running", message: null } },
    prompt: "Review the fixture", model: null, reasoningEffort: null,
  };
  const childMessage = { type: "agentMessage", id: "child-result", text: "Packaged child result", phase: "final_answer" };
  const wait = { ...spawn, id: "wait-fixture", tool: "wait", receiverThreadIds: [],
    agentsStates: { [childThread.id]: { status: "completed", message: childMessage.text } }, prompt: null };
  const event = (method, threadId, turnId, item) => send({ method, params: { threadId, turnId, item } });
  event("item/started", thread.id, turn.id, { ...spawn, status: "inProgress" });
  event("item/completed", thread.id, turn.id, spawn);
  event("item/started", childThread.id, "child-turn", childMessage);
  send({ method: "item/agentMessage/delta", params: {
    threadId: childThread.id, turnId: "child-turn", itemId: childMessage.id, delta: childMessage.text,
  } });
  event("item/completed", childThread.id, "child-turn", childMessage);
  // Native records a child's launch prompt in the child's own history, but never streams it.
  const childInput = { type: "userMessage", id: "child-input", clientId: null,
    content: [{ type: "text", text: spawn.prompt, text_elements: [] }] };
  const childTurn = { id: "child-turn", items: [childInput, childMessage], status: "completed", error: null };
  childThread.turns = [childTurn];
  send({ method: "turn/completed", params: { threadId: childThread.id, turn: childTurn } });
  event("item/started", thread.id, turn.id, { ...wait, status: "inProgress" });
  event("item/completed", thread.id, turn.id, wait);
  const completed = { ...turn, items: [spawn, wait], status: "completed" };
  thread.turns.push(completed);
  send({ method: "turn/completed", params: { threadId: thread.id, turn: completed } });
}
