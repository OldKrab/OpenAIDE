// Moves an embedded adapter submodule to a fork commit and regenerates its runtime
// lockfile (ADR-0079). `update-adapter.yml` runs it for one adapter, then builds,
// tests, and opens a pull request that merges itself on green CI.
//
//   node scripts/update-adapter.mjs <adapter-id>
//   ADAPTER_TARGET_COMMIT  full fork commit to pin; defaults to the fork's `main`
//
// The target must be on the fork's `main`, so only reviewed fork code is embedded.
// Outputs `changed`, `current`, `target`, `short`, `version`, `submodule`,
// `runtime_package`, and `fork_url` for the workflow.

import { spawnSync } from "node:child_process";
import { appendFileSync } from "node:fs";
import process from "node:process";
import { fileURLToPath } from "node:url";

import {
  ensureSubmodule, forkPackage, git, recordedCommit, repositoryRoot, requireCommit, selectAdapters,
} from "./embedded-adapters.mjs";

const FORK_BRANCH = "main";

function emitOutput(name, value) {
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `${name}=${value}\n`);
}

/** The fork's web address from `.gitmodules`, for the pull request's compare link. */
function forkUrl(adapter) {
  const url = git(repositoryRoot, ["config", "--file", ".gitmodules", `submodule.${adapter.submodule}.url`]);
  return url.replace(/\.git$/, "");
}

function update(id) {
  if (!id) throw new Error("usage: update-adapter.mjs <adapter-id>");
  const [adapter] = selectAdapters([id]);
  const started = Date.now();
  const current = recordedCommit(adapter);
  const submodule = ensureSubmodule(adapter, current);
  git(submodule, ["fetch", "--quiet", "origin", `+refs/heads/${FORK_BRANCH}:refs/remotes/origin/${FORK_BRANCH}`]);
  const main = git(submodule, ["rev-parse", `refs/remotes/origin/${FORK_BRANCH}`]);
  const requested = process.env.ADAPTER_TARGET_COMMIT?.trim();
  const target = requested ? requireCommit(requested.toLowerCase(), "requested commit") : main;
  const onMain = spawnSync("git", ["merge-base", "--is-ancestor", target, main], { cwd: submodule, stdio: "ignore" });
  if (onMain.status !== 0) throw new Error(`fork commit ${target} is not on ${FORK_BRANCH}`);

  const changed = target !== current;
  if (changed) {
    git(submodule, ["checkout", "--quiet", "--detach", target]);
    git(repositoryRoot, ["add", "--", adapter.submodule]);
    const lock = spawnSync(process.execPath, [fileURLToPath(new URL("./adapter-lockfile.mjs", import.meta.url)), "--write", adapter.id], {
      cwd: repositoryRoot, stdio: "inherit",
    });
    if (lock.status !== 0) throw new Error("the runtime lockfile could not be regenerated");
    console.log(`${adapter.submodule} moved from ${current.slice(0, 12)} to ${target.slice(0, 12)} in ${Date.now() - started} ms`);
  } else {
    console.log(`${adapter.submodule} is already at ${current.slice(0, 12)}`);
  }
  const { version } = forkPackage(adapter, target);
  emitOutput("changed", changed);
  emitOutput("current", current);
  emitOutput("target", target);
  emitOutput("short", target.slice(0, 12));
  emitOutput("version", version);
  emitOutput("submodule", adapter.submodule);
  emitOutput("runtime_package", adapter.runtimePackage);
  emitOutput("fork_url", forkUrl(adapter));
}

try {
  update(process.argv[2]);
} catch (error) {
  console.error(`update-adapter: ${error.message}`);
  process.exit(1);
}
