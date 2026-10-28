import { readFileSync } from "node:fs";
import test from "node:test";
import assert from "node:assert/strict";

test("web redeploy rebuilds the App Server before restarting the web shell", () => {
  const source = readFileSync(new URL("./redeploy-web-dev.mjs", import.meta.url), "utf8");
  const runtimeBuild = source.indexOf('run("npm", ["run", "app-server:build"])');
  const webDependencyBuild = source.indexOf("buildWebDependencies();");
  const processStop = source.indexOf("stopExistingWebApps();");

  assert.notEqual(runtimeBuild, -1);
  assert.ok(runtimeBuild < webDependencyBuild);
  assert.ok(webDependencyBuild < processStop);
});

test("web redeploy stops only what owns its own ports, never a deployed Web Shell", () => {
  const source = readFileSync(new URL("./redeploy-web-dev.mjs", import.meta.url), "utf8");

  assert.match(source, /OPENAIDE_WEB_PORT \?\? "5384"/);
  assert.match(source, /OPENAIDE_WEB_VITE_PORT \?\? "5383"/);
  // A command match would also stop the Driver and the Agents it runs.
  assert.doesNotMatch(source, /command\.includes\("npm run dev --workspace openaide-web"\)/);
  assert.match(source, /descendantsOf\(processRows, listenerPids\)/);
});
