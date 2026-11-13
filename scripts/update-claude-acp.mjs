// Moves the Claude adapter submodule to a fork commit and regenerates its runtime
// lockfile (ADR-0079). `update-claude-acp.yml` runs it, then builds, tests, and
// opens a pull request that merges itself on green CI.
//
//   CLAUDE_ACP_TARGET_COMMIT  full fork commit to pin; defaults to the fork's `main`
//
// The target must be on the fork's `main`, so only reviewed fork code is embedded.
// Outputs `changed`, `current`, `target`, `short`, and `version` for the workflow.

import { spawnSync } from "node:child_process";
import { appendFileSync } from "node:fs";
import process from "node:process";
import { fileURLToPath } from "node:url";

import {
  ensureSubmodule, forkPackage, git, recordedCommit, repositoryRoot, requireCommit, selectAdapters,
} from "./embedded-adapters.mjs";

const ADAPTER_ID = "claude-agent-acp";
const FORK_BRANCH = "main";

function emitOutput(name, value) {
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `${name}=${value}\n`);
}

function update() {
  const [adapter] = selectAdapters([ADAPTER_ID]);
  const current = recordedCommit(adapter);
  const submodule = ensureSubmodule(adapter, current);
  git(submodule, ["fetch", "--quiet", "origin", `+refs/heads/${FORK_BRANCH}:refs/remotes/origin/${FORK_BRANCH}`]);
  const main = git(submodule, ["rev-parse", `refs/remotes/origin/${FORK_BRANCH}`]);
  const requested = process.env.CLAUDE_ACP_TARGET_COMMIT?.trim();
  const target = requested ? requireCommit(requested.toLowerCase(), "requested commit") : main;
  const onMain = spawnSync("git", ["merge-base", "--is-ancestor", target, main], { cwd: submodule, stdio: "ignore" });
  if (onMain.status !== 0) throw new Error(`fork commit ${target} is not on ${FORK_BRANCH}`);

  const changed = target !== current;
  if (changed) {
    git(submodule, ["checkout", "--quiet", "--detach", target]);
    git(repositoryRoot, ["add", "--", adapter.submodule]);
    const lock = spawnSync(process.execPath, [fileURLToPath(new URL("./adapter-lockfile.mjs", import.meta.url)), "--write", ADAPTER_ID], {
      cwd: repositoryRoot, stdio: "inherit",
    });
    if (lock.status !== 0) throw new Error("the runtime lockfile could not be regenerated");
    console.log(`${adapter.submodule} moved from ${current.slice(0, 12)} to ${target.slice(0, 12)}`);
  } else {
    console.log(`${adapter.submodule} is already at ${current.slice(0, 12)}`);
  }
  const { version } = forkPackage(adapter, target);
  emitOutput("changed", changed);
  emitOutput("current", current);
  emitOutput("target", target);
  emitOutput("short", target.slice(0, 12));
  emitOutput("version", version);
}

try {
  update();
} catch (error) {
  console.error(`update-claude-acp: ${error.message}`);
  process.exit(1);
}
