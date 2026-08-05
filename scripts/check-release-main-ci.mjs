import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

/** Requires the exact checked-out main commit to have a successful completed CI push run. */
export async function requireSuccessfulMainCi({ repository, sha, token, fetchImpl = fetch }) {
  if (!repository || !sha || !token) {
    throw new Error("repository, sha, and token are required to verify main CI");
  }
  const query = new URLSearchParams({
    branch: "main",
    event: "push",
    head_sha: sha,
    status: "completed",
    per_page: "100",
  });
  const response = await fetchImpl(
    `https://api.github.com/repos/${repository}/actions/workflows/ci.yml/runs?${query}`,
    {
      headers: {
        accept: "application/vnd.github+json",
        authorization: `Bearer ${token}`,
        "x-github-api-version": "2022-11-28",
      },
    },
  );
  if (!response.ok) {
    throw new Error(`GitHub CI lookup failed (${response.status}): ${(await response.text()).slice(0, 500)}`);
  }
  const body = await response.json();
  const successful = body.workflow_runs?.find(
    (run) => run.head_sha === sha && run.event === "push" && run.conclusion === "success",
  );
  if (!successful) {
    throw new Error(`main commit ${sha} does not have a successful completed CI push run`);
  }
  return successful;
}

async function main() {
  const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const sha = execFileSync("git", ["rev-parse", "HEAD"], { cwd: repoRoot, encoding: "utf8" }).trim();
  const run = await requireSuccessfulMainCi({
    repository: process.env.GITHUB_REPOSITORY,
    sha,
    token: process.env.GITHUB_TOKEN,
  });
  console.log(`Verified successful CI run ${run.html_url ?? run.id} for main commit ${sha}.`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await main();
}
