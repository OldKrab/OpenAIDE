import assert from "node:assert/strict";
import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";

import { RUNTIME, adapterRepository, git } from "./adapter-repository-fixture.mjs";

function outputs(file) {
  return Object.fromEntries(readFileSync(file, "utf8").trim().split("\n").map((line) => line.split("=")));
}

async function update(fixture, target) {
  const output = path.join(fixture.root, `output-${Math.random().toString(16).slice(2)}`);
  writeFileSync(output, "");
  await fixture.run("update-claude-acp.mjs", [], { GITHUB_OUTPUT: output, CLAUDE_ACP_TARGET_COMMIT: target ?? "" });
  return outputs(output);
}

test("the submodule follows the fork main and its runtime lockfile is regenerated", async (t) => {
  const fixture = adapterRepository(t);
  const second = fixture.commitFork("1.1.0");

  const result = await update(fixture);

  assert.deepEqual(result, {
    changed: "true", current: fixture.first, target: second, short: second.slice(0, 12), version: "1.1.0",
  });
  assert.equal(fixture.recorded(), second);
  const manifest = JSON.parse(readFileSync(path.join(fixture.repo, RUNTIME, "package.json"), "utf8"));
  assert.equal(manifest.openaideAdapter.commit, second);
  await fixture.run("adapter-lockfile.mjs", ["--check"]);
});

test("a current submodule is left unchanged", async (t) => {
  const fixture = adapterRepository(t);

  const result = await update(fixture);

  assert.equal(result.changed, "false");
  assert.equal(fixture.recorded(), fixture.first);
});

test("a requested commit is pinned only when it is a full commit on the fork main", async (t) => {
  const fixture = adapterRepository(t);
  const second = fixture.commitFork("1.1.0");
  fixture.commitFork("1.2.0");
  git(fixture.fork, "switch", "--quiet", "--create", "feature", second);
  const unmerged = fixture.commitFork("1.1.1-feature");
  git(fixture.fork, "switch", "--quiet", "main");

  await assert.rejects(update(fixture, second.slice(0, 12)), /not a full 40-character commit/);
  await assert.rejects(update(fixture, unmerged), /is not on main/);
  assert.equal(fixture.recorded(), fixture.first);

  const result = await update(fixture, second);
  assert.equal(result.target, second);
  assert.equal(result.version, "1.1.0");
  assert.equal(fixture.recorded(), second);
});
