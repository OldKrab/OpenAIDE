#!/usr/bin/env node
// Native Codex restores model/approval history, but derives the resumed sandbox
// from current config. A separate history fixture exposes that distinction.
import { appendFileSync, readFileSync } from "node:fs";
import { createInterface } from "node:readline";

const fixture = JSON.parse(readFileSync(process.env.OPENAIDE_CODEX_RECOVERY_FIXTURE, "utf8"));
const thread = {
  id: "native-session", sessionId: "native-session", preview: "Fixture session",
  createdAt: 1, updatedAt: 1, cwd: fixture.cwd, modelProvider: "openai",
  path: fixture.historyPath ?? null, turns: [], historyMode: "legacy", status: { type: "idle" }, source: "appServer",
};
const childThread = { ...thread, id: "child-fixture", sessionId: "child-fixture", path: null, turns: [] };
const send = (message) => process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", ...message })}\n`);
const model = (id, isDefault) => ({
  id, model: id, displayName: id, description: id, isDefault,
  supportedReasoningEfforts: [{ reasoningEffort: "medium", description: "Medium" }],
  defaultReasoningEffort: "medium", inputModalities: ["text"], supportsPersonality: false,
});
let turnNumber = 0;
let steeringTurn;
let activePolicy = { ...fixture.policy, sandbox: fixture.globalSandbox ?? { type: "dangerFullAccess" } };
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
    case "model/list": result = { data: [model("native-luna", false), model("native-astra", true)], nextCursor: null }; break;
    case "thread/list": result = {
      data: request.params.cursor ? [] : [thread], nextCursor: request.params.cursor ? null : "next-index-page",
    }; break;
    case "thread/resume": {
      const params = request.params;
      activePolicy = {
        approvalPolicy: params.approvalPolicy ?? fixture.policy.approvalPolicy,
        approvalsReviewer: params.approvalsReviewer ?? fixture.policy.approvalsReviewer,
        sandbox: params.permissions ? fixture.policy.sandbox
          : params.sandbox === "read-only" ? { type: "readOnly", networkAccess: false }
            : fixture.globalSandbox ?? { type: "dangerFullAccess" },
      };
      result = {
        thread, model: params.modelProvider ? "native-astra" : "native-luna",
        modelProvider: params.modelProvider ?? "openai", reasoningEffort: "medium",
        serviceTier: null, cwd: fixture.cwd, ...activePolicy,
      };
      break;
    }
    case "thread/settings/update": {
      result = {};
      const params = request.params;
      // ACK admits the change; only the later notification commits it. A prompt
      // sent before that event observes the read-only staging policy.
      setTimeout(() => {
        activePolicy = { approvalPolicy: params.approvalPolicy,
          approvalsReviewer: params.approvalsReviewer, sandbox: params.sandboxPolicy };
        appendFileSync(fixture.calls, `${JSON.stringify({ method: "fixture/settingsCommitted" })}\n`);
        send({ method: "thread/settings/updated", params: { threadId: thread.id,
          threadSettings: { approvalPolicy: params.approvalPolicy, approvalsReviewer: params.approvalsReviewer,
            sandboxPolicy: params.sandboxPolicy, collaborationMode: { mode: "default", settings: {
              model: "native-luna", reasoning_effort: "medium", developer_instructions: null,
            } } } } });
      }, 20);
      break;
    }
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
      const params = request.params;
      appendFileSync(fixture.calls, `${JSON.stringify({ method: "fixture/turnPolicy", params: {
        approvalPolicy: params.approvalPolicy ?? activePolicy.approvalPolicy,
        approvalsReviewer: params.approvalsReviewer ?? activePolicy.approvalsReviewer,
        sandboxPolicy: params.sandboxPolicy ?? activePolicy.sandbox,
      } })}\n`);
      const turn = { id: `turn-fixture-${++turnNumber}`, items: [], status: "inProgress", error: null };
      result = { turn };
      if (fixture.subagents) {
        setImmediate(() => emitSubagentTurn(turn));
        break;
      }
      if (params.input.some((item) => item.text === "Steering race fixture")) {
        steeringTurn = turn;
        break;
      }
      setImmediate(() => send({ method: "turn/completed", params: {
        threadId: thread.id, turn: { ...turn, status: "completed" },
      } }));
      break;
    }
    case "turn/steer": {
      // Complete exactly after the adapter selected the active turn, before
      // native injection. This is the production late-delivery race.
      send({ method: "turn/completed", params: {
        threadId: thread.id, turn: { ...steeringTurn, status: "completed" },
      } });
      steeringTurn = undefined;
      send({ id: request.id, error: { code: -32600, message: "no active turn to steer" } });
      continue;
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
  const childTurn = { id: "child-turn", items: [childMessage], status: "completed", error: null };
  childThread.turns = [childTurn];
  send({ method: "turn/completed", params: { threadId: childThread.id, turn: childTurn } });
  event("item/started", thread.id, turn.id, { ...wait, status: "inProgress" });
  event("item/completed", thread.id, turn.id, wait);
  const completed = { ...turn, items: [spawn, wait], status: "completed" };
  thread.turns.push(completed);
  send({ method: "turn/completed", params: { threadId: thread.id, turn: completed } });
}
