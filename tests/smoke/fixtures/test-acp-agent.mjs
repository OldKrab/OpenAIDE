#!/usr/bin/env node

import readline from "node:readline";

const sessions = new Map();
const pendingClientRequests = new Map();
let nextSession = 1;
let nextClientRequest = 1;

const input = readline.createInterface({ input: process.stdin });
input.on("line", (line) => {
  let message;
  try {
    message = JSON.parse(line);
  } catch (error) {
    log(`invalid input: ${error.message}`);
    return;
  }

  if (message.method) {
    void handleRequestOrNotification(message).catch((error) => {
      log(`${message.method} failed: ${error.stack ?? error.message}`);
      if (message.id !== undefined) respondError(message.id, -32603, error.message);
    });
    return;
  }

  const pending = pendingClientRequests.get(String(message.id));
  if (!pending) return;
  pendingClientRequests.delete(String(message.id));
  if (message.error) pending.reject(new Error(message.error.message ?? "Client request failed"));
  else pending.resolve(message.result);
});

async function handleRequestOrNotification(message) {
  const params = message.params ?? {};
  switch (message.method) {
    case "initialize":
      respond(message.id, {
        protocolVersion: params.protocolVersion ?? 1,
        agentCapabilities: {
          loadSession: true,
          promptCapabilities: { image: true, embeddedContext: true },
          sessionCapabilities: { close: {}, list: {} },
        },
        authMethods: [],
        agentInfo: { name: "OpenAIDE Test Agent", version: "1" },
      });
      break;
    case "session/new":
      createSession(message);
      break;
    case "session/load":
      loadSession(message);
      break;
    case "session/list":
      respond(message.id, { sessions: [] });
      break;
    case "session/set_config_option":
      setConfigOption(message);
      break;
    case "session/prompt":
      // Prompt work remains asynchronous so another prompt, a permission response,
      // or cancellation can cross the same real ACP connection while it is active.
      void runPrompt(message);
      break;
    case "session/cancel":
      cancelSession(params.sessionId);
      break;
    case "session/close":
      sessions.delete(params.sessionId);
      respond(message.id, {});
      break;
    default:
      if (message.id !== undefined) respondError(message.id, -32601, `Unknown method: ${message.method}`);
  }
}

function createSession(message) {
  const sessionId = `smoke-session-${nextSession++}`;
  sessions.set(sessionId, { activePrompts: new Map(), promptCount: 0 });
  respond(message.id, {
    sessionId,
    configOptions: configOptions("balanced"),
  });
  update(sessionId, {
    sessionUpdate: "available_commands_update",
    availableCommands: [
      { name: "permission", description: "Exercise a live ACP permission request." },
      { name: "hold", description: "Keep a prompt active for steering and cancellation." },
    ],
  });
}

function loadSession(message) {
  const sessionId = message.params.sessionId;
  sessions.set(sessionId, { activePrompts: new Map(), promptCount: 0 });
  textUpdate(sessionId, "user_message_chunk", "Earlier question", "replay-user");
  textUpdate(sessionId, "agent_thought_chunk", "Earlier reasoning", "replay-thought");
  textUpdate(sessionId, "agent_message_chunk", "Earlier answer", "replay-agent");
  respond(message.id, { configOptions: configOptions("balanced") });
}

function setConfigOption(message) {
  const value = message.params.value;
  const options = configOptions(value);
  update(message.params.sessionId, {
    sessionUpdate: "config_option_update",
    configOptions: options,
  });
  respond(message.id, { configOptions: options });
}

async function runPrompt(message) {
  const sessionId = message.params.sessionId;
  const session = sessions.get(sessionId);
  if (!session) {
    respondError(message.id, -32000, `Unknown session: ${sessionId}`);
    return;
  }
  session.promptCount += 1;
  const promptNumber = session.promptCount;
  const text = promptText(message.params.prompt);
  const prompt = { id: message.id, cancelled: false };
  session.activePrompts.set(String(message.id), prompt);

  if (text.includes("smoke:hold")) {
    textUpdate(sessionId, "agent_message_chunk", "Waiting for steering", `agent-${promptNumber}`);
    return;
  }
  if (text.includes("smoke:permission")) {
    await permissionScenario(sessionId, message, promptNumber);
    session.activePrompts.delete(String(message.id));
    return;
  }
  if (text.includes("smoke:question")) {
    const answer = await requestClient("elicitation/create", {
      sessionId,
      mode: "form",
      message: "Name the project",
      requestedSchema: {
        type: "object",
        properties: {
          projectName: {
            type: "string",
            title: "Project name",
            description: "Used only by the deterministic smoke scenario.",
            minLength: 1,
          },
        },
        required: ["projectName"],
      },
    });
    const value = answer?.action === "accept" ? answer.content?.projectName : "cancelled";
    textUpdate(sessionId, "agent_message_chunk", `Question result: ${value}`, `agent-${promptNumber}`);
    respond(message.id, { stopReason: "end_turn", userMessageId: message.params.messageId });
    session.activePrompts.delete(String(message.id));
    return;
  }

  if (session.activePrompts.size > 1) {
    textUpdate(sessionId, "agent_message_chunk", `Steering received: ${text}`, `agent-${promptNumber}`);
    respond(message.id, { stopReason: "end_turn", userMessageId: message.params.messageId });
    session.activePrompts.delete(String(message.id));
    return;
  }

  textUpdate(sessionId, "agent_thought_chunk", "Inspecting ", `thought-${promptNumber}`);
  await delay(16);
  textUpdate(sessionId, "agent_thought_chunk", "the request", `thought-${promptNumber}`);
  toolUpdate(sessionId, {
    sessionUpdate: "tool_call",
    toolCallId: `tool-${promptNumber}`,
    title: "Read smoke fixture",
    kind: "read",
    status: "in_progress",
    rawInput: { path: "README.md" },
  });
  await delay(16);
  toolUpdate(sessionId, {
    sessionUpdate: "tool_call_update",
    toolCallId: `tool-${promptNumber}`,
    status: "completed",
    content: [{ type: "content", content: { type: "text", text: "fixture output" } }],
    rawOutput: { ok: true },
  });
  textUpdate(sessionId, "agent_message_chunk", "Smoke ", `agent-${promptNumber}`);
  await delay(16);
  textUpdate(sessionId, "agent_message_chunk", "answer", `agent-${promptNumber}`);
  update(sessionId, { sessionUpdate: "session_info_update", title: "Smoke task" });
  respond(message.id, { stopReason: "end_turn", userMessageId: message.params.messageId });
  session.activePrompts.delete(String(message.id));
}

async function permissionScenario(sessionId, message, promptNumber) {
  const toolCallId = `permission-tool-${promptNumber}`;
  toolUpdate(sessionId, {
    sessionUpdate: "tool_call",
    toolCallId,
    title: "npm test",
    kind: "execute",
    status: "pending",
    rawInput: { command: "npm test" },
  });
  const permission = requestClient("session/request_permission", {
    sessionId,
    toolCall: {
      toolCallId,
      title: "npm test",
      kind: "execute",
      status: "pending",
    },
    options: [
      { optionId: "allow-once", name: "Allow once", kind: "allow_once" },
      { optionId: "reject-once", name: "Reject", kind: "reject_once" },
    ],
  });

  // This update is deliberately sent after the request but before its response.
  // It guards the real race where a Task snapshot used to erase the live request.
  textUpdate(sessionId, "agent_thought_chunk", "Still evaluating permission", `thought-${promptNumber}`);
  textUpdate(sessionId, "agent_message_chunk", "Permission is still pending", `agent-${promptNumber}`);

  const result = await permission;
  const selected = result?.outcome?.outcome === "selected"
    ? result.outcome.optionId
    : "cancelled";
  toolUpdate(sessionId, {
    sessionUpdate: "tool_call_update",
    toolCallId,
    status: selected === "allow-once" ? "completed" : "failed",
    content: [{ type: "content", content: { type: "text", text: `permission:${selected}` } }],
  });
  textUpdate(sessionId, "agent_message_chunk", `Permission result: ${selected}`, `agent-${promptNumber}`);
  respond(message.id, {
    stopReason: selected === "cancelled" ? "cancelled" : "end_turn",
    userMessageId: message.params.messageId,
  });
}

function cancelSession(sessionId) {
  const session = sessions.get(sessionId);
  if (!session) return;
  for (const prompt of session.activePrompts.values()) {
    prompt.cancelled = true;
    respond(prompt.id, { stopReason: "cancelled" });
  }
  session.activePrompts.clear();
}

function configOptions(currentValue) {
  return [{
    id: "test-mode",
    name: "Test mode",
    description: "Deterministic Agent behavior",
    type: "select",
    currentValue,
    options: [
      { value: "balanced", name: "Balanced" },
      { value: "verbose", name: "Verbose" },
    ],
  }];
}

function promptText(blocks = []) {
  return blocks
    .filter((block) => block?.type === "text")
    .map((block) => block.text)
    .join("\n");
}

function textUpdate(sessionId, kind, text, messageId) {
  update(sessionId, {
    sessionUpdate: kind,
    messageId,
    content: { type: "text", text },
  });
}

function toolUpdate(sessionId, payload) {
  update(sessionId, payload);
}

function update(sessionId, payload) {
  notify("session/update", { sessionId, update: payload });
}

function requestClient(method, params) {
  const id = `test-agent-${nextClientRequest++}`;
  write({ jsonrpc: "2.0", id, method, params });
  return new Promise((resolve, reject) => pendingClientRequests.set(id, { resolve, reject }));
}

function notify(method, params) {
  write({ jsonrpc: "2.0", method, params });
}

function respond(id, result) {
  write({ jsonrpc: "2.0", id, result });
}

function respondError(id, code, message) {
  write({ jsonrpc: "2.0", id, error: { code, message } });
}

function write(message) {
  process.stdout.write(`${JSON.stringify(message)}\n`);
}

function log(message) {
  process.stderr.write(`[test-acp-agent] ${message}\n`);
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
