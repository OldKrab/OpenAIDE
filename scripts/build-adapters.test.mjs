import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";

import { ADAPTER_ID, SUBMODULE, adapterRepository, git } from "./adapter-repository-fixture.mjs";

const stage = (repo) => path.join(repo, "adapters/build", ADAPTER_ID);
const manifest = (repo) => JSON.parse(readFileSync(path.join(stage(repo), "manifest.json"), "utf8"));
const digest = (file) => createHash("sha256").update(readFileSync(file)).digest("hex");

test("stages the publishable files of the recorded commit with their digests", async (t) => {
  const fixture = adapterRepository(t);

  await fixture.run("build-adapters.mjs");

  const built = manifest(fixture.repo);
  assert.equal(built.commit, fixture.first);
  assert.equal(built.name, "@openaide/fixture-acp");
  assert.equal(built.version, "1.0.0");
  // The fork's `files` field decides the set: sources and build scripts stay out.
  assert.deepEqual(built.files.map((file) => file.path), ["README.md", "dist/index.js", "package.json"]);
  for (const file of built.files) {
    assert.equal(digest(path.join(stage(fixture.repo), "package", file.path)), file.sha256);
  }
});

test("an unchanged build is skipped and a modified stage is rebuilt", async (t) => {
  const fixture = adapterRepository(t);
  await fixture.run("build-adapters.mjs");

  const repeated = await fixture.run("build-adapters.mjs");
  assert.match(repeated.stdout, /up to date/);

  const index = path.join(stage(fixture.repo), "package/dist/index.js");
  writeFileSync(index, "tampered");
  const rebuilt = await fixture.run("build-adapters.mjs");
  assert.match(rebuilt.stdout, /staged file changed/);
  assert.equal(digest(index), manifest(fixture.repo).files.find((file) => file.path === "dist/index.js").sha256);
});

test("the index pins the build, not the submodule checkout", async (t) => {
  const fixture = adapterRepository(t);
  const second = fixture.commitFork("1.1.0");
  const submodule = path.join(fixture.repo, SUBMODULE);
  git(submodule, "fetch", "--quiet", "origin");
  git(submodule, "checkout", "--quiet", "--detach", second);

  await fixture.run("build-adapters.mjs");
  assert.equal(manifest(fixture.repo).commit, fixture.first);
  assert.match(readFileSync(path.join(stage(fixture.repo), "package/dist/index.js"), "utf8"), /1\.0\.0/);

  git(fixture.repo, "add", SUBMODULE);
  const moved = await fixture.run("build-adapters.mjs");
  assert.match(moved.stdout, /submodule moved/);
  assert.equal(manifest(fixture.repo).commit, second);
  assert.equal(manifest(fixture.repo).version, "1.1.0");
});

test("a fresh clone initializes its adapter submodule", async (t) => {
  const fixture = adapterRepository(t);
  const clone = path.join(fixture.root, "clone");
  git(fixture.root, "clone", "--quiet", fixture.repo, clone);

  await fixture.run("build-adapters.mjs", [], { OPENAIDE_REPOSITORY_ROOT: clone });

  assert.equal(manifest(clone).commit, fixture.first);
});

test("an unknown adapter id fails", async (t) => {
  const fixture = adapterRepository(t);
  await assert.rejects(fixture.run("build-adapters.mjs", ["missing-acp"]), /unknown adapter missing-acp/);
});

test("a linked worktree initializes its own submodule and stage", async (t) => {
  const fixture = adapterRepository(t);
  const worktree = path.join(fixture.root, "worktree");
  git(fixture.repo, "worktree", "add", "--quiet", "--detach", worktree);

  await fixture.run("build-adapters.mjs", [], { OPENAIDE_REPOSITORY_ROOT: worktree });

  assert.equal(manifest(worktree).commit, fixture.first);
});
