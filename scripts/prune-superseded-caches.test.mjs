import assert from "node:assert/strict";
import test from "node:test";
import { cacheFamily, pruneSupersededCaches, supersededCaches } from "./prune-superseded-caches.mjs";

const main = "refs/heads/main";
const nodeHash = (digit) => digit.repeat(64);
let nextId = 1;
const cache = (key, created_at, { ref = main, size = 100 } = {}) => ({
  id: nextId++, key, ref, created_at, size_in_bytes: size,
});

test("a family is what a later run restores from", () => {
  assert.equal(cacheFamily("v0-rust-quality-Linux-x64-d92a5e34-9a317f16"), "v0-rust-quality-Linux-x64-d92a5e34");
  assert.equal(cacheFamily("v0-rust-win32-x64-build-Windows_NT-x64-beef5405-5b18c107"), "v0-rust-win32-x64-build-Windows_NT-x64-beef5405");
  assert.equal(cacheFamily(`node-cache-Linux-x64-npm-${nodeHash("f")}`), "node-cache-Linux-x64-npm");
  // setup-gradle manages its own entries.
  assert.equal(cacheFamily("gradle-dependencies-v2-c95a62169c7641e90b9c6dfc97ac1e44"), null);
});

test("only main's caches replaced by a newer one of their family are superseded", () => {
  const oldRust = cache("v0-rust-rust-Linux-x64-d92a5e34-cc68b406", "2026-11-24T12:21:00Z", { size: 900 });
  const newRust = cache("v0-rust-rust-Linux-x64-d92a5e34-9a317f16", "2026-11-24T12:36:00Z");
  const otherEnvironment = cache("v0-rust-rust-Linux-x64-0000aaaa-cc68b406", "2026-11-20T10:00:00Z");
  const oldNode = cache(`node-cache-Linux-x64-npm-${nodeHash("a")}`, "2026-11-20T10:00:00Z");
  const newNode = cache(`node-cache-Linux-x64-npm-${nodeHash("b")}`, "2026-11-21T10:00:00Z");
  const pullRequest = cache("v0-rust-rust-Linux-x64-d92a5e34-11111111", "2026-11-24T12:00:00Z", { ref: "refs/pull/811/merge" });
  const gradle = cache("gradle-dependencies-v2-c95a62169c7641e90b9c6dfc97ac1e44", "2026-11-01T00:00:00Z");

  const superseded = supersededCaches([newRust, oldRust, otherEnvironment, oldNode, newNode, pullRequest, gradle]);

  assert.deepEqual(superseded.map(({ id }) => id).sort(), [oldRust.id, oldNode.id].sort());
  assert.equal(superseded.find(({ id }) => id === oldRust.id).family, "v0-rust-rust-Linux-x64-d92a5e34");
});

function fakeGitHub(pages, deleteStatus = () => 204) {
  const requests = [];
  return {
    requests,
    async fetchImpl(url, options = {}) {
      requests.push({ url, method: options.method ?? "GET", authorization: options.headers.authorization });
      if (options.method === "DELETE") {
        const status = deleteStatus(Number(url.split("/").pop()));
        return new Response(null, { status });
      }
      const page = Number(new URL(url).searchParams.get("page"));
      const total = pages.flat().length;
      return Response.json({ total_count: total, actions_caches: pages[page - 1] ?? [] });
    },
  };
}

test("lists every page, deletes the superseded caches, and reports what it freed", async () => {
  const kept = cache("v0-rust-build-Linux-x64-d92a5e34-9a317f16", "2026-11-24T12:36:00Z");
  const replaced = cache("v0-rust-build-Linux-x64-d92a5e34-cc68b406", "2026-11-24T12:21:00Z", { size: 955 });
  const evicted = cache("v0-rust-build-Linux-x64-d92a5e34-0badf00d", "2026-11-23T12:21:00Z", { size: 400 });
  const refused = cache(`node-cache-Linux-x64-npm-${nodeHash("a")}`, "2026-11-20T10:00:00Z", { size: 50 });
  const newestNode = cache(`node-cache-Linux-x64-npm-${nodeHash("b")}`, "2026-11-21T10:00:00Z");
  const github = fakeGitHub([[kept, replaced], [evicted, refused, newestNode]], (id) => (
    id === evicted.id ? 404 : id === refused.id ? 403 : 204
  ));
  const events = [];

  const summary = await pruneSupersededCaches({
    repository: "owner/repo", token: "test-token", fetchImpl: github.fetchImpl, onEvent: (event) => events.push(event),
  });

  assert.deepEqual(summary, {
    listed: 5, pages: 2, superseded: 3, superseded_bytes: 1405, deleted: 1, gone: 1, failed: 1, freed_bytes: 955,
  });
  assert.deepEqual(
    github.requests.filter(({ method }) => method === "DELETE").map(({ url }) => Number(url.split("/").pop())).sort(),
    [replaced.id, evicted.id, refused.id].sort(),
  );
  assert.ok(github.requests.every(({ authorization }) => authorization === "Bearer test-token"));
  assert.match(github.requests[0].url, /ref=refs%2Fheads%2Fmain/);
  assert.deepEqual(events.map(({ operation, outcome }) => `${operation}:${outcome}`).sort(), [
    "cache_prune:failed", "cache_prune:started",
    "cache_prune_delete:deleted", "cache_prune_delete:failed", "cache_prune_delete:gone",
  ]);
  // A refused deletion fails the run so the workflow shows it; the others still ran.
  assert.equal(events.at(-1).phase, "delete");
  assert.equal(events.find(({ outcome }) => outcome === "deleted").size_bytes, 955);
});

test("a dry run lists what it would delete and deletes nothing", async () => {
  const github = fakeGitHub([[
    cache("v0-rust-smoke-tests-Linux-x64-d92a5e34-9a317f16", "2026-11-24T12:36:00Z"),
    cache("v0-rust-smoke-tests-Linux-x64-d92a5e34-cc68b406", "2026-11-24T12:21:00Z"),
  ]]);
  const events = [];

  const summary = await pruneSupersededCaches({
    repository: "owner/repo", token: "test-token", dryRun: true, fetchImpl: github.fetchImpl, onEvent: (event) => events.push(event),
  });

  assert.equal(summary.superseded, 1);
  assert.equal(summary.deleted, 0);
  assert.ok(github.requests.every(({ method }) => method === "GET"));
  assert.ok(events.some(({ outcome }) => outcome === "skipped_dry_run"));
  assert.equal(events.at(-1).outcome, "completed");
});

test("a refused listing fails with its status and deletes nothing", async () => {
  const events = [];
  const requests = [];
  await assert.rejects(
    pruneSupersededCaches({
      repository: "owner/repo",
      token: "test-token",
      async fetchImpl(url, options = {}) {
        requests.push(options.method ?? "GET");
        return new Response("forbidden", { status: 403 });
      },
      onEvent: (event) => events.push(event),
    }),
    /cache list failed \(403\)/,
  );
  assert.deepEqual(requests, ["GET"]);
  assert.deepEqual(
    { outcome: events.at(-1).outcome, phase: events.at(-1).phase, status: events.at(-1).status, error_kind: events.at(-1).error_kind },
    { outcome: "failed", phase: "list", status: 403, error_kind: "http" },
  );
});
