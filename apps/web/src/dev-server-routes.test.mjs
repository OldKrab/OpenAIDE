import assert from "node:assert/strict";
import test from "node:test";
import { appServerTransportRoute, injectBootstrap, webRoute } from "./dev-server-routes.mjs";

test("recognizes direct app routes that need web shell bootstrap metadata", () => {
  assert.deepEqual(webRoute("/"), { archived: undefined, surface: "task", taskId: undefined });
  assert.deepEqual(webRoute("/new-task"), { archived: undefined, surface: "task", taskId: undefined });
  assert.deepEqual(webRoute("/archive"), { archived: true, surface: "task", taskId: undefined });
  assert.deepEqual(webRoute("/settings"), { archived: undefined, surface: "settings", taskId: undefined });
  assert.deepEqual(webRoute("/task/task_1"), { archived: undefined, surface: "task", taskId: "task_1" });
  assert.deepEqual(webRoute("/session/codex/session%2F1"), {
    agentId: "codex",
    archived: undefined,
    nativeSessionId: "session/1",
    surface: "nativeSession",
    taskId: undefined,
  });
});

test("injects Native Session identity into direct opening-route loads", () => {
  const html = '<html><body><div id="root"></div></body></html>';
  const injected = injectBootstrap(html, webRoute("/session/codex/session_1"));

  assert.match(injected, /data-surface="nativeSession"/);
  assert.match(injected, /data-agent-id="codex"/);
  assert.match(injected, /data-native-session-id="session_1"/);
});

test("does not treat Vite or App Server support paths as app routes", () => {
  assert.equal(webRoute("/@vite/client"), undefined);
  assert.equal(webRoute("/src/main.tsx"), undefined);
  assert.equal(webRoute("/__openaide-app-server/probe"), undefined);
});

test("streams both whole-file and chunked upload routes to the matching App Server path", () => {
  assert.deepEqual(appServerTransportRoute("POST", "/__openaide-app-server/upload"), {
    kind: "upload",
    appServerSuffix: "upload",
  });
  assert.deepEqual(appServerTransportRoute("POST", "/__openaide-app-server/upload/chunk"), {
    kind: "upload",
    appServerSuffix: "upload/chunk",
  });
  assert.deepEqual(appServerTransportRoute("GET", "/__openaide-app-server/download"), {
    kind: "download",
    appServerSuffix: "download",
  });
});

test("injects web shell connection metadata into direct archive loads", () => {
  const html = '<html><body><div id="root"></div></body></html>';
  const injected = injectBootstrap(html, webRoute("/archive"));

  assert.match(
    injected,
    /<body data-shell="web" data-navigation-mode="project" data-surface="task" data-archived="true"/,
  );
  assert.match(injected, /data-app-server-connection="[^"]*&quot;kind&quot;:&quot;webProxy&quot;/);
});

test("injects instance label and title for distinguishable deployed instances", () => {
  const html = '<html><head><title>OpenAIDE</title></head><body><div id="root"></div></body></html>';
  const injected = injectBootstrap(html, webRoute("/"), {
    instanceLabel: "Target",
    title: "OpenAIDE Target",
  });

  assert.match(injected, /<title>OpenAIDE Target<\/title>/);
  assert.match(injected, /data-instance-label="Target"/);
});

test("escapes task ids before injecting route metadata", () => {
  const html = '<html><body><div id="root"></div></body></html>';
  const injected = injectBootstrap(html, { surface: "task", taskId: 'task"<bad>' });

  assert.match(injected, /data-task-id="task&quot;&lt;bad&gt;"/);
});
