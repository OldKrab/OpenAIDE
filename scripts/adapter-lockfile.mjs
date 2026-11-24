// The runtime `package.json` and `package-lock.json` App Server installs an
// embedded adapter's dependencies from (ADR-0079). Both are generated from the
// fork's `package.json` at the commit the superproject records.
//
//   npm run adapters:lock [id...]   regenerate them after moving a submodule
//   npm run adapters:check [id...]  fail when they no longer match the submodule
//
// The check is offline: the manifest must equal the generated one byte for byte,
// and the lockfile must lock every direct dependency at a version the manifest
// allows, which is what `npm ci` itself requires.

import { copyFileSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import process from "node:process";
import semver from "semver";

import {
  ensureSubmodule, forkPackage, formatJson, npm, recordedCommit, runtimeManifest, runtimePaths, selectAdapters,
} from "./embedded-adapters.mjs";

function expectedManifest(adapter) {
  const commit = recordedCommit(adapter);
  ensureSubmodule(adapter, commit);
  return runtimeManifest(adapter, forkPackage(adapter, commit), commit);
}

/** Problems that make the committed runtime files disagree with the submodule. */
function lockProblems(adapter, manifest) {
  const paths = runtimePaths(adapter);
  if (!existsSync(paths.packageJson)) return ["package.json is missing"];
  if (readFileSync(paths.packageJson, "utf8") !== formatJson(manifest)) {
    return ["package.json does not match the submodule's package.json and commit"];
  }
  if (!existsSync(paths.lockfile)) return ["package-lock.json is missing"];
  const lock = JSON.parse(readFileSync(paths.lockfile, "utf8"));
  const problems = [];
  if (lock.name !== manifest.name) problems.push("package-lock.json names another package");
  if (JSON.stringify(lock.packages?.[""]?.dependencies ?? {}) !== JSON.stringify(manifest.dependencies)) {
    problems.push("package-lock.json root dependencies differ from package.json");
  }
  for (const [name, range] of Object.entries(manifest.dependencies)) {
    const locked = lock.packages?.[`node_modules/${name}`]?.version;
    if (!locked || !semver.satisfies(locked, range)) problems.push(`package-lock.json does not lock ${name}@${range}`);
  }
  // The adapter is unpacked by App Server; npm must never install a published copy.
  if (lock.packages?.[`node_modules/${manifest.openaideAdapter.name}`]) {
    problems.push(`package-lock.json installs ${manifest.openaideAdapter.name} itself`);
  }
  return problems;
}

function check(adapters) {
  let failed = false;
  for (const adapter of adapters) {
    const problems = lockProblems(adapter, expectedManifest(adapter));
    for (const problem of problems) console.error(`adapters:check ${adapter.id}: ${adapter.runtimePackage}/${problem}`);
    failed ||= problems.length > 0;
  }
  if (failed) {
    console.error("Run `npm run adapters:lock` and commit the result.");
    process.exit(1);
  }
  console.log("adapters:check passed: runtime lockfiles match their submodules");
}

/**
 * Locks in a scratch directory seeded with the committed lockfile, so unchanged
 * dependencies keep their resolution and npm never sees this repository's workspaces.
 */
function write(adapters) {
  for (const adapter of adapters) {
    const manifest = expectedManifest(adapter);
    const paths = runtimePaths(adapter);
    const scratch = mkdtempSync(path.join(tmpdir(), `openaide-${adapter.id}-lock-`));
    try {
      writeFileSync(path.join(scratch, "package.json"), formatJson(manifest));
      if (existsSync(paths.lockfile)) copyFileSync(paths.lockfile, path.join(scratch, "package-lock.json"));
      npm(["install", "--package-lock-only", "--ignore-scripts", "--no-audit", "--no-fund"], scratch, `${adapter.id} lock`);
      writeFileSync(paths.packageJson, formatJson(manifest));
      copyFileSync(path.join(scratch, "package-lock.json"), paths.lockfile);
    } finally {
      rmSync(scratch, { recursive: true, force: true });
    }
    const problems = lockProblems(adapter, manifest);
    if (problems.length > 0) throw new Error(`${adapter.id}: ${problems.join("; ")}`);
    const { version, commit } = manifest.openaideAdapter;
    console.log(`adapters:lock ${adapter.id}: locked the dependencies of ${version} (${commit.slice(0, 12)})`);
  }
}

const [mode, ...ids] = process.argv.slice(2);
try {
  if (mode === "--check") check(selectAdapters(ids));
  else if (mode === "--write") write(selectAdapters(ids));
  else throw new Error("usage: adapter-lockfile.mjs --check|--write [id...]");
} catch (error) {
  console.error(`adapter lockfile: ${error.message}`);
  process.exit(1);
}
