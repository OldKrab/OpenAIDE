import assert from "node:assert/strict";
import test from "node:test";
import { requireSuccessfulMainCi } from "./check-release-main-ci.mjs";

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
