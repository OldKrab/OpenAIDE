import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

const script = path.join(import.meta.dirname, "assert-windows-gui-executable.mjs");

test("accepts a Windows GUI executable", async () => {
  await withPeFixture(2, (fixture) => {
    const result = spawnSync(process.execPath, [script, fixture], { encoding: "utf8" });
    assert.equal(result.status, 0, result.stderr);
  });
});

test("rejects a Windows console executable", async () => {
  await withPeFixture(3, (fixture) => {
    const result = spawnSync(process.execPath, [script, fixture], { encoding: "utf8" });
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /Windows GUI subsystem/);
  });
});

async function withPeFixture(subsystem, assertion) {
  const root = await mkdtemp(path.join(os.tmpdir(), "openaide-pe-fixture-"));
  try {
    const fixture = path.join(root, "desktop.exe");
    const image = Buffer.alloc(512);
    image.write("MZ", 0, "ascii");
    image.writeUInt32LE(0x80, 0x3c);
    image.write("PE\0\0", 0x80, "binary");
    image.writeUInt16LE(0x20b, 0x80 + 24);
    image.writeUInt16LE(subsystem, 0x80 + 24 + 68);
    await writeFile(fixture, image);
    assertion(fixture);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}
