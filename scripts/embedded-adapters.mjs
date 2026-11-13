// The built-in Agent adapters App Server embeds from fork submodules (ADR-0079).
//
// `adapters/adapters.json` lists them. The superproject's gitlink is the only pin:
// every script here reads the fork at the commit the index records, never the
// submodule's working tree, so a developer's local checkout inside a submodule
// cannot leak into a build, a lockfile, or a check.

import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

export const repositoryRoot = path.resolve(
  process.env.OPENAIDE_REPOSITORY_ROOT ?? fileURLToPath(new URL("..", import.meta.url)),
);

const COMMIT_PATTERN = /^[0-9a-f]{40}$/;
const ID_PATTERN = /^[a-z0-9][a-z0-9-]*$/;

export function loadAdapters(root = repositoryRoot) {
  const config = JSON.parse(readFileSync(path.join(root, "adapters/adapters.json"), "utf8"));
  if (!Array.isArray(config.adapters)) throw new Error("adapters/adapters.json must list adapters");
  return config.adapters.map((adapter) => {
    if (!ID_PATTERN.test(adapter.id ?? "")) throw new Error(`invalid adapter id ${JSON.stringify(adapter.id)}`);
    for (const field of ["submodule", "runtimePackage"]) {
      if (typeof adapter[field] !== "string" || adapter[field].length === 0) {
        throw new Error(`adapter ${adapter.id} is missing ${field}`);
      }
    }
    if (!Array.isArray(adapter.build) || adapter.build.length === 0) {
      throw new Error(`adapter ${adapter.id} is missing its build command`);
    }
    const buildScripts = adapter.buildScripts ?? [];
    if (!Array.isArray(buildScripts) || !buildScripts.every((name) => typeof name === "string" && name.length > 0)) {
      throw new Error(`adapter ${adapter.id} buildScripts must list package names`);
    }
    return { ...adapter, buildScripts };
  });
}

/** Selects adapters by id; no ids selects all of them. */
export function selectAdapters(ids, root = repositoryRoot) {
  const adapters = loadAdapters(root);
  if (ids.length === 0) return adapters;
  return ids.map((id) => {
    const adapter = adapters.find((candidate) => candidate.id === id);
    if (!adapter) throw new Error(`unknown adapter ${id}`);
    return adapter;
  });
}

export function git(root, args, options = {}) {
  const output = execFileSync("git", args, { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"], ...options });
  return (output ?? "").trim();
}

/** The fork commit the superproject index records for this adapter. */
export function recordedCommit(adapter, root = repositoryRoot) {
  const entry = git(root, ["ls-files", "--stage", "--", adapter.submodule]);
  const match = /^160000 ([0-9a-f]{40}) 0\t/.exec(entry);
  if (!match) throw new Error(`${adapter.submodule} is not a submodule in the index`);
  return match[1];
}

export function requireCommit(value, source) {
  if (typeof value !== "string" || !COMMIT_PATTERN.test(value)) {
    throw new Error(`${source} is not a full 40-character commit`);
  }
  return value;
}

function hasCommit(submodule, commit) {
  if (!existsSync(path.join(submodule, ".git"))) return false;
  return spawnSync("git", ["cat-file", "-e", `${commit}^{commit}`], { cwd: submodule, stdio: "ignore" }).status === 0;
}

/**
 * Makes the recorded commit readable from the submodule. A fresh clone or worktree
 * gets the submodule initialized; an initialized submodule is left on whatever the
 * developer checked out, and only fetches when it lacks the commit.
 */
export function ensureSubmodule(adapter, commit, root = repositoryRoot) {
  const submodule = path.join(root, adapter.submodule);
  if (hasCommit(submodule, commit)) return submodule;
  if (!existsSync(path.join(submodule, ".git"))) {
    git(root, ["submodule", "update", "--init", "--", adapter.submodule], { stdio: "inherit" });
  } else {
    git(submodule, ["fetch", "--quiet", "origin", commit], { stdio: "inherit" });
  }
  if (!hasCommit(submodule, commit)) {
    throw new Error(`${adapter.submodule} does not contain the recorded commit ${commit}`);
  }
  return submodule;
}

/** Reads one file of the fork at a commit, independent of the submodule's checkout. */
export function forkFile(adapter, commit, file, root = repositoryRoot) {
  return git(path.join(root, adapter.submodule), ["show", `${commit}:${file}`], { maxBuffer: 64 * 1024 * 1024 });
}

export function forkPackage(adapter, commit, root = repositoryRoot) {
  return JSON.parse(forkFile(adapter, commit, "package.json", root));
}

/**
 * Writes the fork's tree at `commit` into `destination` without touching the
 * submodule's HEAD, index, or working tree. Line endings stay as committed so a
 * Windows build stages the same bytes as a Linux one.
 */
export function exportCommit(adapter, commit, destination, root = repositoryRoot) {
  const submodule = path.join(root, adapter.submodule);
  rmSync(destination, { recursive: true, force: true });
  mkdirSync(destination, { recursive: true });
  const env = { ...process.env, GIT_INDEX_FILE: path.join(destination, "..", `.${path.basename(destination)}.index`) };
  try {
    git(submodule, ["read-tree", commit], { env });
    git(submodule, ["-c", "core.autocrlf=false", "-c", "core.eol=lf", "checkout-index", "--all", "--force",
      // Git for Windows accepts drive paths with forward slashes everywhere.
      `--prefix=${destination.split(path.sep).join("/")}/`], { env });
  } finally {
    rmSync(env.GIT_INDEX_FILE, { force: true });
  }
}

/**
 * The runtime manifest App Server installs with `npm ci`: the fork's runtime
 * dependencies, and the adapter identity the catalog, Android, and the App Server
 * build check read. The adapter itself is not a dependency; App Server unpacks
 * its embedded copy beside them.
 */
export function runtimeManifest(adapter, fork, commit) {
  if (typeof fork.name !== "string" || typeof fork.version !== "string") {
    throw new Error(`${adapter.submodule} package.json has no name or version`);
  }
  return {
    name: `openaide-${adapter.id}-runtime`,
    private: true,
    description: `Generated by npm run adapters:lock from ${adapter.submodule}; do not edit.`,
    openaideAdapter: { name: fork.name, version: fork.version, commit },
    dependencies: { ...(fork.dependencies ?? {}) },
  };
}

export function formatJson(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

export function runtimePaths(adapter, root = repositoryRoot) {
  const directory = path.join(root, adapter.runtimePackage);
  return {
    directory,
    packageJson: path.join(directory, "package.json"),
    lockfile: path.join(directory, "package-lock.json"),
  };
}

export function stagedPaths(adapter, root = repositoryRoot) {
  const directory = path.join(root, "adapters/build", adapter.id);
  return {
    directory,
    packageDirectory: path.join(directory, "package"),
    manifest: path.join(directory, "manifest.json"),
    work: path.join(root, "adapters/build/.work", adapter.id),
  };
}

/** Runs npm (a `.cmd` launcher on Windows) and fails with the step name. */
export function npm(args, cwd, step) {
  const result = spawnSync("npm", args, { cwd, shell: process.platform === "win32", stdio: "inherit" });
  if (result.status !== 0) throw new Error(`${step} failed (npm ${args[0]} exited with ${result.status ?? result.signal})`);
}
