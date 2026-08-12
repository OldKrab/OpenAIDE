import assert from "node:assert/strict";
import test from "node:test";
import { requireSuccessfulMainCi, waitForSuccessfulMainCi } from "./check-release-main-ci.mjs";

test("accepts a successful CI push run for the exact main commit", async () => {
  const requests = [];
  const run = await requireSuccessfulMainCi({
    repository: "OldKrab/OpenAIDE",
    sha: "abc123",
    token: "test-token",
    async fetchImpl(url, options) {
      requests.push({ url, options });
      return Response.json({ workflow_runs: [
        { id: 42, head_sha: "abc123", event: "push", conclusion: "success" },
      ] });
    },
  });

  assert.equal(run.id, 42);
  assert.match(requests[0].url, /head_sha=abc123/);
  assert.equal(requests[0].options.headers.authorization, "Bearer test-token");
});

test("rejects missing, stale, and unsuccessful CI runs", async () => {
  await assert.rejects(
    requireSuccessfulMainCi({
      repository: "OldKrab/OpenAIDE",
      sha: "current",
      token: "test-token",
      async fetchImpl() {
        return Response.json({ workflow_runs: [
          { head_sha: "stale", event: "push", conclusion: "success" },
          { head_sha: "current", event: "push", conclusion: "failure" },
        ] });
      },
    }),
    /does not have a successful completed CI/,
  );
});

test("surfaces GitHub API failures without treating them as a failed check", async () => {
  await assert.rejects(
    requireSuccessfulMainCi({
      repository: "OldKrab/OpenAIDE",
      sha: "current",
      token: "test-token",
      async fetchImpl() {
        return new Response("rate limited", { status: 403 });
      },
    }),
    /lookup failed \(403\): rate limited/,
  );
});

test("waits for the tagged main commit CI without rerunning the suite", async () => {
  const responses = [
    { status: "in_progress", conclusion: null },
    { status: "completed", conclusion: "success" },
  ];
  let waits = 0;
  const run = await waitForSuccessfulMainCi({
    repository: "OldKrab/OpenAIDE",
    sha: "release-sha",
    token: "test-token",
    attempts: 3,
    async wait() { waits += 1; },
    async fetchImpl() {
      const state = responses.shift();
      return Response.json({ workflow_runs: [{
        id: 84,
        head_sha: "release-sha",
        event: "push",
        ...state,
      }] });
    },
  });

  assert.equal(run.id, 84);
  assert.equal(waits, 1);
});

test("stops waiting when the tagged main commit CI fails", async () => {
  await assert.rejects(
    waitForSuccessfulMainCi({
      repository: "OldKrab/OpenAIDE",
      sha: "release-sha",
      token: "test-token",
      async wait() { throw new Error("must not wait after terminal failure"); },
      async fetchImpl() {
        return Response.json({ workflow_runs: [{
          id: 85,
          head_sha: "release-sha",
          event: "push",
          status: "completed",
          conclusion: "failure",
        }] });
      },
    }),
    /completed with failure/,
  );
});
