import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

/** Requires the exact checked-out main commit to have a successful completed CI push run. */
export async function requireSuccessfulMainCi({ repository, sha, token, fetchImpl = fetch }) {
  const run = await lookupMainCi({ repository, sha, token, fetchImpl });
  if (run?.conclusion !== "success") {
    throw new Error(`main commit ${sha} does not have a successful completed CI push run`);
  }
  return run;
}

/** Waits for normal main CI so release artifact builds can run concurrently instead of repeating it. */
export async function waitForSuccessfulMainCi({
  repository,
  sha,
  token,
  fetchImpl = fetch,
  attempts = 120,
  wait = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds)),
}) {
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    const run = await lookupMainCi({ repository, sha, token, fetchImpl });
    if (run?.conclusion === "success") return run;
    if (run?.status === "completed") {
      throw new Error(`main commit ${sha} CI completed with ${run.conclusion ?? "an unknown conclusion"}`);
    }
    if (attempt + 1 < attempts) await wait(5_000);
  }
  throw new Error(`main commit ${sha} CI did not complete successfully within 10 minutes`);
}

async function lookupMainCi({ repository, sha, token, fetchImpl }) {
  if (!repository || !sha || !token) {
    throw new Error("repository, sha, and token are required to verify main CI");
  }
  const query = new URLSearchParams({
    branch: "main",
    event: "push",
    head_sha: sha,
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
  return body.workflow_runs?.find(
    (run) => run.head_sha === sha && run.event === "push",
  );
}

async function main() {
  const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const sha = execFileSync("git", ["rev-parse", "HEAD"], { cwd: repoRoot, encoding: "utf8" }).trim();
  const check = process.argv.includes("--wait") ? waitForSuccessfulMainCi : requireSuccessfulMainCi;
  const run = await check({
    repository: process.env.GITHUB_REPOSITORY,
    sha,
    token: process.env.GITHUB_TOKEN,
  });
  console.log(`Verified successful CI run ${run.html_url ?? run.id} for main commit ${sha}.`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await main();
}
