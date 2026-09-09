#!/usr/bin/env node
// Native 0.153.3 restores model/approval history, but derives the resumed sandbox
// from current config. A separate history fixture exposes that distinction.
import { appendFileSync, readFileSync } from "node:fs";
import { createInterface } from "node:readline";

const fixture = JSON.parse(readFileSync(process.env.OPENAIDE_CODEX_RECOVERY_FIXTURE, "utf8"));
const thread = {
  id: "native-session", sessionId: "native-session", preview: "Fixture session",
  createdAt: 1, updatedAt: 1, cwd: fixture.cwd, modelProvider: "openai",
  path: fixture.historyPath ?? null, turns: [], status: { type: "idle" }, source: "appServer",
};
const send = (message) => process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", ...message })}\n`);
const model = (id, isDefault) => ({
  id, model: id, displayName: id, description: id, isDefault,
  supportedReasoningEfforts: [{ reasoningEffort: "medium", description: "Medium" }],
  defaultReasoningEffort: "medium", inputModalities: ["text"], supportsPersonality: false,
});
let turnNumber = 0;
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
    case "thread/read": result = { thread }; break;
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
      setImmediate(() => send({ method: "turn/completed", params: {
        threadId: thread.id, turn: { ...turn, status: "completed" },
      } }));
      break;
    }
    default:
      send({ id: request.id, error: { code: -32601, message: `Unsupported fixture method: ${request.method}` } });
      continue;
  }
  send({ id: request.id, result });
}
