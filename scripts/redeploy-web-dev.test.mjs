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
