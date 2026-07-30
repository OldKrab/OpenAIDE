import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  chmodSync,
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";

const prototypeRoot = path.dirname(new URL(import.meta.url).pathname);

test("launcher installs workspace dependencies in a clean checkout", () => {
  const tempRoot = mkdtempSync(path.join(tmpdir(), "openaide-desktop-launcher-"));
  const fakeBin = path.join(tempRoot, "bin");
  const repoRoot = path.join(tempRoot, "repo");
  const testPrototypeRoot = path.join(
    repoRoot,
    "tmp",
    "prototypes",
    "desktop-tauri",
  );
  const callsFile = path.join(tempRoot, "calls.log");

  try {
    mkdirSync(fakeBin, { recursive: true });
    mkdirSync(testPrototypeRoot, { recursive: true });
    copyFileSync(path.join(prototypeRoot, "run.sh"), path.join(testPrototypeRoot, "run.sh"));
    writeExecutable(
      path.join(fakeBin, "cargo"),
      `#!/usr/bin/env bash\nprintf 'cargo|%s|%s\\n' "$PWD" "$*" >> "${callsFile}"\n`,
    );
    writeExecutable(
      path.join(fakeBin, "npm"),
      `#!/usr/bin/env bash\nprintf 'npm|%s|%s\\n' "$PWD" "$*" >> "${callsFile}"\n`,
    );

    const result = spawnSync("bash", [path.join(testPrototypeRoot, "run.sh")], {
      encoding: "utf8",
      env: {
        ...process.env,
        PATH: `${fakeBin}${path.delimiter}${process.env.PATH}`,
      },
    });

    assert.equal(result.status, 0, result.stderr);
    assert.deepEqual(readFileSync(callsFile, "utf8").trim().split("\n"), [
      `npm|${repoRoot}|install`,
      `cargo|${repoRoot}|build -p openaide-app-server`,
      `npm|${testPrototypeRoot}|install`,
      `npm|${testPrototypeRoot}|run tauri -- dev`,
    ]);
  } finally {
    rmSync(tempRoot, { recursive: true, force: true });
  }
});

function writeExecutable(file, contents) {
  writeFileSync(file, contents);
  chmodSync(file, 0o755);
}
