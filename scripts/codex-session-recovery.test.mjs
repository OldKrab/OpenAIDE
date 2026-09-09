import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { execFile } from "node:child_process";
import { copyFile, mkdir, mkdtemp, readFile, rm, writeFile, access } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import test from "node:test";

const run = promisify(execFile);
const assets = fileURLToPath(new URL("../openaide-rs/app-server/assets/codex-acp-runtime/", import.meta.url));
const digest = (value) => createHash("sha256").update(value).digest("hex");

async function fixture(t) {
  const root = await mkdtemp(path.join(os.tmpdir(), "openaide-codex-patch-test-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const packageRoot = path.join(root, "node_modules/@openaide/codex-acp");
  await mkdir(path.join(packageRoot, "dist"), { recursive: true });
  const manifest = JSON.parse(await readFile(path.join(assets, "session-recovery-manifest.json"), "utf8"));
  for (const file of ["apply-session-recovery.mjs", "session-recovery.mjs", "session-recovery-manifest.json"]) {
    await copyFile(path.join(assets, file), path.join(root, file));
  }
  await writeFile(path.join(packageRoot, "package.json"), JSON.stringify({ name: manifest.packageName, version: manifest.packageVersion }));
  const entrypoint = path.join(packageRoot, "dist/index.js");
  const helper = path.join(packageRoot, "dist/openaide-session-recovery.mjs");
  const script = path.join(root, "apply-session-recovery.mjs");
  return { root, packageRoot, entrypoint, helper, manifest, execute: (...args) => run(process.execPath, [script, ...args], { timeout: 5000 }) };
}

test("rejected package versions cannot rewrite a staged adapter", async (t) => {
  const setup = await fixture(t);
  const original = "unrecognized adapter bytes";
  await writeFile(setup.entrypoint, original);
  await writeFile(path.join(setup.packageRoot, "package.json"), JSON.stringify({ name: setup.manifest.packageName, version: "99.0.0" }));

  await assert.rejects(setup.execute(setup.root));

  assert.equal(await readFile(setup.entrypoint, "utf8"), original);
  await assert.rejects(access(setup.helper));
});

test("a matching package version cannot patch an unrecognized upstream bundle", async (t) => {
  const setup = await fixture(t);
  const original = "unrecognized adapter bytes";
  await writeFile(setup.entrypoint, original);

  await assert.rejects(setup.execute(setup.root));

  assert.equal(await readFile(setup.entrypoint, "utf8"), original);
  await assert.rejects(access(setup.helper));
});

test("artifact verification rejects corruption of either independently pinned file", async (t) => {
  const setup = await fixture(t);
  // Tiny known artifacts keep this verifier boundary offline. The actual pinned
  // npm bundle is exercised by smoke-packaged-codex-acp after provisioning.
  const index = "export const version = 1;";
  const helper = "export const policy = 2;";
  await writeFile(setup.entrypoint, index);
  await writeFile(setup.helper, helper);
  await writeFile(path.join(setup.root, "session-recovery-manifest.json"), JSON.stringify({
    ...setup.manifest, patchedSha256: digest(index), helperSha256: digest(helper),
  }));
  await setup.execute("--verify", setup.root);

  await writeFile(setup.entrypoint, index + "broken");
  await assert.rejects(setup.execute("--verify", setup.root));
  await writeFile(setup.entrypoint, index);
  await writeFile(setup.helper, helper + "broken");
  await assert.rejects(setup.execute("--verify", setup.root));
});

test("the dependency updater recognizes the current patched version without changing files", async (t) => {
  const setup = await fixture(t);
  const config = path.join(setup.root, "agent-config.rs");
  const source = `const PIN: &str = "${setup.manifest.packageName}@${setup.manifest.packageVersion}";`;
  await writeFile(config, source);

  await run(process.execPath, [fileURLToPath(new URL("./update-codex-acp-version.mjs", import.meta.url))], {
    env: { ...process.env, CODEX_ACP_CONFIG_PATH: config, CODEX_ACP_LATEST_VERSION: setup.manifest.packageVersion },
    timeout: 5000,
  });

  assert.equal(await readFile(config, "utf8"), source);
});

test("a newer dependency version requires patch review before updating any pin", async (t) => {
  const setup = await fixture(t);
  const config = path.join(setup.root, "agent-config.rs");
  const source = `const PIN: &str = "${setup.manifest.packageName}@${setup.manifest.packageVersion}";`;
  await writeFile(config, source);

  await assert.rejects(run(process.execPath, [fileURLToPath(new URL("./update-codex-acp-version.mjs", import.meta.url))], {
    env: { ...process.env, CODEX_ACP_CONFIG_PATH: config, CODEX_ACP_LATEST_VERSION: "99.0.0" },
    timeout: 5000,
  }));

  assert.equal(await readFile(config, "utf8"), source);
});
