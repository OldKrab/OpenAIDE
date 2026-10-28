import assert from "node:assert/strict";
import test from "node:test";

import { appServerEnvironment } from "./app-server-environment.mjs";

test("the App Server gets its roots but none of the Web Shell's own settings", () => {
  const env = appServerEnvironment({
    PATH: "/bin",
    OPENAIDE_SMOKE_GATE_ROOT: "/gates",
    OPENAIDE_WEB_PORT: "5474",
    OPENAIDE_WEB_STATE_ROOT: "/driver/state",
    OPENAIDE_WEB_PROJECT_ROOTS: "/projects",
    OPENAIDE_APP_SERVER_PATH: "/driver/openaide-app-server",
    OPENAIDE_RUNTIME_PATH: "/legacy/openaide-app-server",
    OPENAIDE_PROJECT_ROOTS: "/ignored",
  }, { stateRoot: "/state", runtimeRoot: "/runtime" });

  assert.deepEqual(env, {
    PATH: "/bin",
    OPENAIDE_SMOKE_GATE_ROOT: "/gates",
    OPENAIDE_PROJECT_ROOTS: "/projects",
    OPENAIDE_STORAGE_ROOT: "/state",
    OPENAIDE_RUNTIME_ROOT: "/runtime",
    OPENAIDE_APP_SERVER_PROTOCOL: "app-server-handoff",
  });
});

test("without Web project roots the App Server gets none", () => {
  const env = appServerEnvironment({ OPENAIDE_PROJECT_ROOTS: "/ignored" }, { stateRoot: "/s", runtimeRoot: "/r" });
  assert.equal("OPENAIDE_PROJECT_ROOTS" in env, false);
});
