import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { smokeNotificationCompanionVsix } from "./smoke-notification-companion-vsix.mjs";

test("verifies the universal notification companion package", async () => {
  const fixture = await fixturePackage();
  try {
    await smokeNotificationCompanionVsix({
      extensionRoot: fixture.extensionRoot,
      version: "0.0.2-beta.1",
    });
  } finally {
    await rm(fixture.root, { recursive: true, force: true });
  }
});

test("rejects a companion package with a mismatched version", async () => {
  const fixture = await fixturePackage();
  try {
    await assert.rejects(
      smokeNotificationCompanionVsix({ extensionRoot: fixture.extensionRoot, version: "0.0.2-beta.2" }),
      /does not match/,
    );
  } finally {
    await rm(fixture.root, { recursive: true, force: true });
  }
});

async function fixturePackage() {
  const root = await mkdtemp(path.join(os.tmpdir(), "openaide-notification-companion-vsix-"));
  const extensionRoot = path.join(root, "extension");
  await mkdir(path.join(extensionRoot, "dist"), { recursive: true });
  await writeFile(path.join(extensionRoot, "package.json"), JSON.stringify({
    name: "openaide-vscode-notification-companion",
    publisher: "openaide",
    version: "0.0.2-beta.1",
  }));
  for (const relativePath of ["dist/extension.js", "readme.md", "LICENSE.txt"]) {
    await writeFile(path.join(extensionRoot, relativePath), "fixture");
  }
  return { root, extensionRoot };
}
