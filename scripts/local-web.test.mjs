import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";

const repoRoot = path.resolve(new URL("..", import.meta.url).pathname);
const script = path.join(repoRoot, "deploy/local-web.sh");

test("local web status reports the running static root recorded by the wrapper", () => {
  const tempDir = mkdtempSync(path.join(tmpdir(), "openaide-local-web-test-"));
  try {
    const pidFile = path.join(tempDir, "web.pid");
    const runningStaticRoot = path.join(tempDir, "target-static");
    const callerStaticRoot = path.join(tempDir, "caller-static");
    writeFileSync(pidFile, `${process.pid}\n`);
    writeFileSync(`${pidFile}.static-root`, `${runningStaticRoot}\n`);

    const result = spawnSync("bash", [script, "status"], {
      cwd: repoRoot,
      encoding: "utf8",
      env: {
        ...process.env,
        OPENAIDE_WEB_ALLOWED_HOSTS: "localhost,127.0.0.1",
        OPENAIDE_WEB_PID_FILE: pidFile,
        OPENAIDE_WEB_PORT: "45974",
        OPENAIDE_WEB_STATIC_ROOT: callerStaticRoot,
        OPENAIDE_WEB_VITE_PORT: "45973",
      },
    });
    const output = result.stdout;

    assert.equal(result.status, 0);

    assert.match(output, new RegExp(`static root: ${escapeRegExp(runningStaticRoot)}`));
    assert.doesNotMatch(output, new RegExp(`static root: ${escapeRegExp(callerStaticRoot)}`));
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
});

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
