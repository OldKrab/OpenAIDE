#!/usr/bin/env node

const baseUrl = new URL(process.argv[2] ?? process.env.OPENAIDE_WEB_SMOKE_URL ?? "http://127.0.0.1:5474");
const endpoint = new URL("/__openaide-app-server/probe", baseUrl);
const totalTimeoutMs = numberFromEnv("OPENAIDE_WEB_SMOKE_TIMEOUT_MS", 60_000);
const sendTimeoutMs = numberFromEnv("OPENAIDE_WEB_SMOKE_SEND_TIMEOUT_MS", 45_000);
const prompt = process.env.OPENAIDE_WEB_SMOKE_PROMPT
  ?? "OpenAIDE redeploy smoke check. Reply with exactly: ok";
const cleanup = process.env.OPENAIDE_WEB_SMOKE_CLEANUP ?? "archive";
const connectionId = `local-web-smoke-${Date.now()}`;

let nextRequestId = 1;
let taskId;
let turnId;

try {
  await withOverallTimeout(runSmoke(), totalTimeoutMs);
  console.log("OpenAIDE local web smoke passed.");
} catch (error) {
  console.error(`OpenAIDE local web smoke failed: ${errorMessage(error)}`);
  process.exitCode = 1;
} finally {
  await cleanupTask();
}

async function runSmoke() {
  const initialized = await request("client/initialize", {
    clientInstanceId: connectionId,
    shell: { kind: "web", name: "local-web-smoke" },
    requestedSurface: { kind: "home" },
    capabilities: {
      protocol: ["requestResponses", "stableClientRequestIds", "resync"],
      shell: [],
    },
  });

  const projectId = initialized.snapshot?.projects?.activeProjectId
    ?? initialized.snapshot?.projects?.projects?.[0]?.projectId;
  if (!projectId) throw new Error("no project is available");

  const agentId = initialized.snapshot?.agents?.defaultAgentId
    ?? initialized.snapshot?.agents?.agents?.[0]?.agentId;
  if (!agentId) throw new Error("no agent is available");

  const created = await request("task/acquire", { projectId, agentId });
  taskId = created.task?.task?.taskId;
  if (!taskId) {
    throw new Error("task/acquire returned an invalid task snapshot");
  }
  await waitUntilTaskSendReady(created.task);

  const sent = await request(
    "task/send",
    {
      taskId,
      message: { text: prompt, attachments: [] },
    },
    sendTimeoutMs,
  );
  turnId = sent.turnId;
  if (!turnId || !sent.userMessageId) {
    throw new Error("task/send returned an invalid send result");
  }
}

async function waitUntilTaskSendReady(initialSnapshot) {
  const deadline = Date.now() + sendTimeoutMs;
  let snapshot = initialSnapshot;
  while (snapshot?.sendCapability?.state !== "ready") {
    if (Date.now() >= deadline) {
      throw new Error(`task/acquire did not become send-ready within ${sendTimeoutMs}ms`);
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
    const opened = await request("task/open", { taskId }, Math.min(5_000, deadline - Date.now()));
    snapshot = opened.task;
  }
}

async function cleanupTask() {
  if (!taskId || cleanup === "none") return;
  await request("task/cancel", { taskId, ...(turnId ? { turnId } : {}) }, 10_000).catch(() => {});
  await request("task/setArchived", { taskId, archived: true }, 10_000).catch(() => {});
}

async function request(method, params, timeoutMs = 15_000) {
  const id = `local-web-smoke-request-${nextRequestId++}`;
  const response = await fetchWithTimeout({
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "X-OpenAIDE-Connection-Id": connectionId,
    },
    body: JSON.stringify({ jsonrpc: "2.0", id, method, params }),
  }, timeoutMs);
  const text = await response.text();
  if (!response.ok) {
    throw new Error(`HTTP ${response.status}: ${text.trim().slice(0, 240)}`);
  }
  const messages = asArray(JSON.parse(text));
  const message = messages.find((item) => item?.jsonrpc === "2.0" && item.id === id);
  if (!message) throw new Error(`${method} did not return a JSON-RPC response`);
  if (message.error) {
    const detail = message.error.message ?? JSON.stringify(message.error);
    throw new Error(`${method} failed: ${detail}`);
  }
  return unwrapEnvelope(message.result);
}

async function fetchWithTimeout(init, timeoutMs) {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), timeoutMs);
  try {
    return await fetch(endpoint, { ...init, signal: controller.signal });
  } catch (error) {
    if (error?.name === "AbortError") {
      throw new Error(`request timed out after ${timeoutMs}ms`);
    }
    throw error;
  } finally {
    clearTimeout(timeout);
  }
}

function unwrapEnvelope(value) {
  if (value && typeof value === "object" && Object.hasOwn(value, "result")) {
    return value.result;
  }
  return value;
}

function asArray(value) {
  return Array.isArray(value) ? value : [value];
}

function numberFromEnv(name, fallback) {
  const raw = process.env[name];
  if (!raw) return fallback;
  const parsed = Number(raw);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

function withOverallTimeout(promise, timeoutMs) {
  let timeout;
  const timeoutPromise = new Promise((_, reject) => {
    timeout = setTimeout(() => reject(new Error(`smoke timed out after ${timeoutMs}ms`)), timeoutMs);
  });
  return Promise.race([promise, timeoutPromise]).finally(() => clearTimeout(timeout));
}

function errorMessage(error) {
  return error instanceof Error ? error.message : String(error);
}
