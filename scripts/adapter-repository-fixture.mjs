// A throwaway superproject with one adapter fork submodule, for the tests of the
// embedded adapter scripts. Everything is local: the fork has no dependencies, so
// npm never needs the registry.

import { execFile, execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const scripts = path.dirname(fileURLToPath(import.meta.url));

export const ADAPTER_ID = "claude-agent-acp";
export const SUBMODULE = `adapters/${ADAPTER_ID}`;
export const RUNTIME = `runtime/${ADAPTER_ID}`;

const gitEnv = {
  GIT_AUTHOR_NAME: "Fixture", GIT_AUTHOR_EMAIL: "fixture@example.com",
  GIT_COMMITTER_NAME: "Fixture", GIT_COMMITTER_EMAIL: "fixture@example.com",
  // Local submodule URLs are file transport, which git disables by default.
  GIT_CONFIG_COUNT: "2", GIT_CONFIG_KEY_0: "protocol.file.allow", GIT_CONFIG_VALUE_0: "always",
  GIT_CONFIG_KEY_1: "init.defaultBranch", GIT_CONFIG_VALUE_1: "main",
};

export function git(cwd, ...args) {
  return execFileSync("git", args, { cwd, encoding: "utf8", env: { ...process.env, ...gitEnv }, stdio: ["ignore", "pipe", "pipe"] }).trim();
}

/** Commits a fork revision with `version` and `dependencies`; returns its commit. */
function commitFork(fork, version, dependencies = {}) {
  const name = "@openaide/fixture-acp";
  writeFileSync(path.join(fork, "package.json"), `${JSON.stringify({
    name, version, type: "module",
    files: ["dist/", "README.md", "package.json"],
    scripts: { build: "node build.mjs" },
    dependencies,
  }, null, 2)}\n`);
  writeFileSync(path.join(fork, "package-lock.json"), JSON.stringify({
    name, version, lockfileVersion: 3, requires: true, packages: { "": { name, version } },
  }));
  writeFileSync(path.join(fork, "build.mjs"),
    `import { mkdirSync, writeFileSync } from "node:fs";\nmkdirSync("dist", { recursive: true });\nwriteFileSync("dist/index.js", ${JSON.stringify(`export const version = "${version}";\n`)});\n`);
  writeFileSync(path.join(fork, "README.md"), `# Fixture ${version}\n`);
  git(fork, "add", "--all");
  git(fork, "commit", "--quiet", "--message", `release ${version}`);
  return git(fork, "rev-parse", "HEAD");
}

export function adapterRepository(t) {
  const root = mkdtempSync(path.join(tmpdir(), "openaide-adapters-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const fork = path.join(root, "fork");
  mkdirSync(fork);
  git(fork, "init", "--quiet");
  const first = commitFork(fork, "1.0.0");

  const repo = path.join(root, "repo");
  mkdirSync(path.join(repo, "adapters"), { recursive: true });
  mkdirSync(path.join(repo, RUNTIME), { recursive: true });
  git(repo, "init", "--quiet");
  writeFileSync(path.join(repo, "adapters/adapters.json"), JSON.stringify({ adapters: [
    { id: ADAPTER_ID, submodule: SUBMODULE, runtimePackage: RUNTIME, build: ["npm", "run", "build"] },
  ] }));
  git(repo, "submodule", "add", "--quiet", fork, SUBMODULE);
  git(repo, "add", "--all");
  git(repo, "commit", "--quiet", "--message", "fixture");

  const env = { ...process.env, ...gitEnv, OPENAIDE_REPOSITORY_ROOT: repo };
  return {
    root, fork, repo, first,
    commitFork: (version, dependencies) => commitFork(fork, version, dependencies),
    recorded: (at = repo) => /^160000 ([0-9a-f]{40}) /.exec(git(at, "ls-files", "--stage", "--", SUBMODULE))[1],
    run: (script, args = [], extra = {}) => execFileAsync(process.execPath, [path.join(scripts, script), ...args], {
      cwd: repo, env: { ...env, ...extra }, timeout: 30_000,
    }),
  };
}
