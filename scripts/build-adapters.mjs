// `npm run build:adapters [id...]`: builds each fork submodule at its recorded
// commit and stages the exact file set `npm pack` would publish, plus a manifest
// with per-file SHA-256 digests, under `adapters/build/<id>/`. The App Server's
// build script embeds that directory and refuses to build without it (ADR-0079).
//
// A build is skipped when the staged manifest names the recorded commit and every
// staged file still matches its digest, so repeated backend builds stay fast.

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";

import {
  ensureSubmodule, exportCommit, formatJson, npm, recordedCommit, repositoryRoot, selectAdapters, stagedPaths,
} from "./embedded-adapters.mjs";

// Bump when the staging recipe changes so existing staged output is rebuilt.
const MANIFEST_SCHEMA = 1;

function sha256(file) {
  return createHash("sha256").update(readFileSync(file)).digest("hex");
}

function listFiles(directory, prefix = "") {
  if (!existsSync(directory)) return [];
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const relative = prefix ? `${prefix}/${entry.name}` : entry.name;
    return entry.isDirectory() ? listFiles(path.join(directory, entry.name), relative) : [relative];
  });
}

/** Why the staged output cannot be reused, or undefined when it is current. */
function staleReason(adapter, commit, root = repositoryRoot) {
  const staged = stagedPaths(adapter, root);
  if (!existsSync(staged.manifest)) return "not built";
  let manifest;
  try {
    manifest = JSON.parse(readFileSync(staged.manifest, "utf8"));
  } catch {
    return "manifest unreadable";
  }
  if (manifest.schema !== MANIFEST_SCHEMA) return "staging recipe changed";
  if (manifest.commit !== commit) return "submodule moved";
  if (JSON.stringify(manifest.build) !== JSON.stringify(adapter.build)) return "build command changed";
  const expected = new Map(manifest.files.map((file) => [file.path, file.sha256]));
  const present = listFiles(staged.packageDirectory);
  if (present.length !== expected.size) return "staged file set changed";
  for (const file of present) {
    if (expected.get(file) !== sha256(path.join(staged.packageDirectory, file))) return "staged file changed";
  }
  return undefined;
}

function run(command, cwd, step) {
  const [program, ...args] = command;
  const result = spawnSync(program, args, { cwd, shell: process.platform === "win32", stdio: "inherit" });
  if (result.status !== 0) throw new Error(`${step} failed (exit ${result.status ?? result.signal})`);
}

/** The publishable file set, as `npm pack` computes it from the fork's `files` field. */
function packedFiles(source) {
  const result = spawnSync("npm", ["pack", "--dry-run", "--json", "--ignore-scripts", "--silent"], {
    cwd: source, encoding: "utf8", shell: process.platform === "win32", maxBuffer: 64 * 1024 * 1024,
  });
  if (result.status !== 0) throw new Error(`listing the publishable files failed (exit ${result.status})`);
  const [pack] = JSON.parse(result.stdout);
  return pack.files.map((file) => file.path.split(path.sep).join("/")).sort();
}

function buildAdapter(adapter, root = repositoryRoot) {
  const started = Date.now();
  const commit = recordedCommit(adapter, root);
  const short = commit.slice(0, 12);
  ensureSubmodule(adapter, commit, root);
  const reason = staleReason(adapter, commit, root);
  if (!reason) {
    console.log(`build:adapters ${adapter.id}: up to date at ${short}`);
    return;
  }
  console.log(`build:adapters ${adapter.id}: building ${short} (${reason})`);

  const staged = stagedPaths(adapter, root);
  const source = path.join(staged.work, "source");
  exportCommit(adapter, commit, source, root);
  // Optional dependencies are the native agent binaries; compiling the adapter
  // does not need them. Lifecycle scripts stay off as in the runtime install.
  npm(["ci", "--ignore-scripts", "--omit=optional", "--no-audit", "--no-fund"], source, `${adapter.id} dependency install`);
  run(adapter.build, source, `${adapter.id} build`);
  const pkg = JSON.parse(readFileSync(path.join(source, "package.json"), "utf8"));
  const files = packedFiles(source);
  if (!files.includes("package.json")) throw new Error(`${adapter.id} would not publish its package.json`);

  // Replace the staged output; the manifest is written last, so an interrupted
  // build is never mistaken for a current one.
  rmSync(staged.manifest, { force: true });
  rmSync(staged.packageDirectory, { recursive: true, force: true });
  const entries = files.map((file) => {
    const target = path.join(staged.packageDirectory, file);
    mkdirSync(path.dirname(target), { recursive: true });
    copyFileSync(path.join(source, file), target);
    const bytes = readFileSync(target);
    return { path: file, size: bytes.length, sha256: createHash("sha256").update(bytes).digest("hex") };
  });
  const manifest = {
    schema: MANIFEST_SCHEMA,
    id: adapter.id,
    name: pkg.name,
    version: pkg.version,
    commit,
    build: adapter.build,
    files: entries,
  };
  writeFileSync(`${staged.manifest}.tmp`, formatJson(manifest));
  renameSync(`${staged.manifest}.tmp`, staged.manifest);
  rmSync(staged.work, { recursive: true, force: true });
  const seconds = ((Date.now() - started) / 1000).toFixed(1);
  console.log(`build:adapters ${adapter.id}: staged ${pkg.name}@${pkg.version} (${short}), ${entries.length} files in ${seconds}s`);
}

try {
  for (const adapter of selectAdapters(process.argv.slice(2))) buildAdapter(adapter);
} catch (error) {
  console.error(`build:adapters failed: ${error.message}`);
  process.exit(1);
}
