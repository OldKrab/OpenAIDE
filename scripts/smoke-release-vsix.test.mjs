import assert from "node:assert/strict";
import { chmod, mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { smokeReleaseVsix } from "./smoke-release-vsix.mjs";

test("verifies required VSIX content and a graceful bundled App Server lifecycle", async () => {
  const fixture = await fixtureVsix();
  try {
    await smokeReleaseVsix({
      extensionRoot: fixture.extensionRoot,
      version: "0.0.2-beta.1",
      target: "linux-x64",
      binaryName: "openaide-app-server",
    });
  } finally {
    await rm(fixture.root, { recursive: true, force: true });
  }
});

test("rejects missing package assets before publication", async () => {
  const fixture = await fixtureVsix();
  try {
    await rm(path.join(fixture.extensionRoot, "webview", "dist", "assets", "index.js"));
    await assert.rejects(
      smokeReleaseVsix({
        extensionRoot: fixture.extensionRoot,
        version: "0.0.2-beta.1",
        target: "linux-x64",
        binaryName: "openaide-app-server",
      }),
      /index\.js/,
    );
  } finally {
    await rm(fixture.root, { recursive: true, force: true });
  }
});

async function fixtureVsix() {
  const root = await mkdtemp(path.join(os.tmpdir(), "openaide-vsix-fixture-"));
  const extensionRoot = path.join(root, "extension");
  await mkdir(path.join(extensionRoot, "dist", "app-server"), { recursive: true });
  await mkdir(path.join(extensionRoot, "webview", "dist", "assets"), { recursive: true });
  await writeFile(path.join(extensionRoot, "package.json"), JSON.stringify({
    name: "openaide-vscode-extension",
    publisher: "openaide",
    version: "0.0.2-beta.1",
  }));
  for (const relativePath of ["LICENSE.txt", "dist/extension.js", "webview/dist/assets/index.js", "webview/dist/assets/index.css"]) {
    await writeFile(path.join(extensionRoot, relativePath), "fixture");
  }
  await writeFile(
    path.join(root, "extension.vsixmanifest"),
    '<Property Id="Microsoft.VisualStudio.Code.TargetPlatform" Value="linux-x64" />',
  );
  const binary = path.join(extensionRoot, "dist", "app-server", "openaide-app-server");
  await writeFile(
    binary,
    '#!/bin/sh\nprintf \'%s\\n\' \'{"jsonrpc":"2.0","id":"release-smoke-shutdown","result":{}}\'\ncat >/dev/null\n',
  );
  await chmod(binary, 0o755);
  return { root, extensionRoot };
}
