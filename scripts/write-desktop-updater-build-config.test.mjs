import assert from "node:assert/strict";
import { existsSync, mkdtempSync, readFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { writeDesktopUpdaterBuildConfig } from "./write-desktop-updater-build-config.mjs";

test("writes the release public key into the Tauri updater build configuration", () => {
  const outputPath = path.join(
    mkdtempSync(path.join(os.tmpdir(), "openaide-updater-config-")),
    "updater.json",
  );
  const publicKey = Buffer.from(
    "untrusted comment: minisign public key: 0000000000000000\nRWRBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFB",
    "utf8",
  ).toString("base64");

  writeDesktopUpdaterBuildConfig(outputPath, publicKey);

  assert.deepEqual(JSON.parse(readFileSync(outputPath, "utf8")), {
    plugins: {
      updater: {
        pubkey: publicKey,
      },
    },
  });
});

test("rejects a malformed release public key before writing build configuration", () => {
  const outputPath = path.join(
    mkdtempSync(path.join(os.tmpdir(), "openaide-updater-config-")),
    "updater.json",
  );
  const missingComment = Buffer.from(
    "RWRBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFB",
    "utf8",
  ).toString("base64");

  assert.throws(
    () => writeDesktopUpdaterBuildConfig(outputPath, missingComment),
    /must decode to a minisign public key/,
  );
  assert.equal(existsSync(outputPath), false);
});
