import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, statSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import {
  ARCHIVE_ENTRIES,
  ARCHIVE_FOLDER,
  appServerArchiveName,
  packageAppServerArchive,
} from "./package-app-server-archive.mjs";

function temporaryDirectory() {
  return mkdtempSync(path.join(os.tmpdir(), "openaide-app-server-archive-test-"));
}

function stubBinary() {
  const binaryPath = path.join(temporaryDirectory(), "openaide-app-server");
  // Written without the executable bit, the way a downloaded build artifact arrives.
  writeFileSync(binaryPath, "#!/bin/sh\necho stub\n", { mode: 0o644 });
  return binaryPath;
}

test("packs the executable, guide, license and version into one folder", () => {
  const outputDirectory = temporaryDirectory();

  const archivePath = packageAppServerArchive({
    binaryPath: stubBinary(),
    version: "1.2.3",
    outputDirectory,
  });

  assert.equal(path.basename(archivePath), "openaide-app-server-linux-x64-1.2.3.tar.gz");
  assert.equal(path.basename(archivePath), appServerArchiveName("1.2.3"));
  const listed = execFileSync("tar", ["-tzf", archivePath], { encoding: "utf8" })
    .split("\n")
    .filter((entry) => entry && !entry.endsWith("/"))
    .sort();
  assert.deepEqual(listed, ARCHIVE_ENTRIES.map((entry) => `${ARCHIVE_FOLDER}/${entry}`));

  const unpacked = temporaryDirectory();
  execFileSync("tar", ["-xzf", archivePath, "-C", unpacked]);
  const folder = path.join(unpacked, ARCHIVE_FOLDER);
  assert.equal(readFileSync(path.join(folder, "VERSION"), "utf8"), "1.2.3\n");
  assert.ok(statSync(path.join(folder, "openaide-app-server")).mode & 0o111);
  assert.equal(execFileSync(path.join(folder, "openaide-app-server"), { encoding: "utf8" }), "stub\n");
  assert.match(readFileSync(path.join(folder, "README.md"), "utf8"), /openaide-app-server remote/);
});

test("records no builder account in the archive", () => {
  const archivePath = packageAppServerArchive({
    binaryPath: stubBinary(),
    version: "1.2.3-rc.1",
    outputDirectory: temporaryDirectory(),
  });

  const owners = execFileSync("tar", ["--numeric-owner", "-tvzf", archivePath], { encoding: "utf8" })
    .split("\n")
    .filter(Boolean)
    .map((line) => line.split(/\s+/)[1]);
  assert.deepEqual([...new Set(owners)], ["0/0"]);
});

test("rejects a version that is not a release version", () => {
  assert.throws(
    () => packageAppServerArchive({
      binaryPath: stubBinary(),
      version: "v1.2.3",
      outputDirectory: temporaryDirectory(),
    }),
    /SemVer without a v prefix/,
  );
});
