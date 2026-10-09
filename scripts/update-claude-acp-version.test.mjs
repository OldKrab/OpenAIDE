import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const run = promisify(execFile);
const script = fileURLToPath(new URL("./update-claude-acp-version.mjs", import.meta.url));
const packageName = "@openaide/claude-agent-acp";

async function fixture(t, version) {
  const root = await mkdtemp(path.join(tmpdir(), "openaide-claude-acp-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const policy = path.join(root, "package.json");
  const output = path.join(root, "output");
  await writeFile(policy, `${JSON.stringify({ dependencies: { [packageName]: version } }, null, 2)}\n`);
  await writeFile(output, "");
  const execute = (env) => run(process.execPath, [script], {
    env: { ...process.env, CLAUDE_ACP_MANIFEST_PATH: policy, GITHUB_OUTPUT: output, ...env },
    timeout: 30_000,
  });
  const pinned = async () => JSON.parse(await readFile(policy, "utf8")).dependencies[packageName];
  return { execute, output, pinned };
}

test("a current pin is left unchanged", async (t) => {
  const setup = await fixture(t, "1.2.3");
  await setup.execute({ CLAUDE_ACP_PUBLISHED_VERSION: "1.2.3" });
  assert.equal(await setup.pinned(), "1.2.3");
  assert.match(await readFile(setup.output, "utf8"), /^changed=false$/m);
});

test("a newer published version replaces the exact pin", async (t) => {
  const setup = await fixture(t, "1.2.3");
  await setup.execute({ CLAUDE_ACP_PUBLISHED_VERSION: "1.3.0" });
  assert.equal(await setup.pinned(), "1.3.0");
  assert.match(await readFile(setup.output, "utf8"), /^changed=true\ncurrent=1\.2\.3\nlatest=1\.3\.0$/m);
});

test("a requested version must be exact and match the registry", async (t) => {
  const setup = await fixture(t, "1.2.3");
  await assert.rejects(setup.execute({ CLAUDE_ACP_TARGET_VERSION: "^1.3.0", CLAUDE_ACP_PUBLISHED_VERSION: "1.3.0" }));
  await assert.rejects(setup.execute({ CLAUDE_ACP_TARGET_VERSION: "1.3.0", CLAUDE_ACP_PUBLISHED_VERSION: "1.4.0" }));
  assert.equal(await setup.pinned(), "1.2.3");
});
