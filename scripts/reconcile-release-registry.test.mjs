import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import {
  collectReleasePackages,
  lookupMarketplaceDigest,
  lookupOpenVsxDigest,
  reconcileRegistry,
} from "./reconcile-release-registry.mjs";

test("reads a target-specific Marketplace digest", async () => {
  const expected = "a".repeat(64);
  const digest = await lookupMarketplaceDigest({
    extensionId: "openaide.openaide-vscode-extension",
    version: "0.0.2",
    target: "linux-x64",
    async fetchImpl(_url, options) {
      assert.equal(JSON.parse(options.body).flags, 17);
      return Response.json({ results: [{ extensions: [{
        publisher: { publisherName: "openaide" },
        extensionName: "openaide-vscode-extension",
        versions: [{
          version: "0.0.2",
          targetPlatform: "linux-x64",
          properties: [{ key: "Microsoft.VisualStudio.Services.VsixSha256", value: expected.toUpperCase() }],
        }],
      }] }] });
    },
  });
  assert.equal(digest, expected);
});

test("treats Open VSX 404 as missing and reads its published checksum", async () => {
  assert.equal(await lookupOpenVsxDigest({
    extensionId: "openaide.openaide-vscode-extension",
    version: "0.0.2",
    target: "linux-x64",
    async fetchImpl() { return new Response("missing", { status: 404 }); },
  }), undefined);

  const digest = await lookupOpenVsxDigest({
    extensionId: "openaide.openaide-vscode-extension",
    version: "0.0.2",
    target: "linux-x64",
    async fetchImpl(url) {
      if (url.endsWith("sha256")) return new Response(`${"d".repeat(64)}\n`);
      return Response.json({ files: { sha256: "https://open-vsx.test/file.sha256" } });
    },
  });
  assert.equal(digest, "d".repeat(64));
});

test("checks once, skips identical packages, publishes missing packages, and rejects conflicts", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "openaide-registry-fixture-"));
  const packagePath = path.join(root, "package.vsix");
  await writeFile(packagePath, "canonical-package");
  const expected = "9c3a0bc8902e99fcabc3dec4f9f6a1c1cb532aa943ede87722ae7374ae3e73d9";
  try {
    let published = 0;
    await reconcileRegistry({
      registry: "marketplace",
      version: "0.0.2",
      packages: [{ target: "linux-x64", path: packagePath }],
      async fetchImpl() { return marketplaceResponse(expected); },
      async publish() { published += 1; },
    });
    assert.equal(published, 0);

    let lookups = 0;
    await reconcileRegistry({
      registry: "marketplace",
      version: "0.0.2",
      packages: [{ target: "linux-x64", path: packagePath }],
      async fetchImpl() {
        lookups += 1;
        return marketplaceResponse(undefined);
      },
      async publish() { published += 1; },
      async wait() { throw new Error("must not wait after accepted publication"); },
    });
    assert.equal(published, 1);
    assert.equal(lookups, 1);

    await assert.rejects(
      reconcileRegistry({
        registry: "marketplace",
        version: "0.0.2",
        packages: [{ target: "linux-x64", path: packagePath }],
        async fetchImpl() { return marketplaceResponse("f".repeat(64)); },
        async publish() { throw new Error("must not publish"); },
      }),
      /has digest f{64}/,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("publishes independent platform packages concurrently", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "openaide-registry-fixture-"));
  const linuxPath = path.join(root, "linux.vsix");
  const windowsPath = path.join(root, "windows.vsix");
  await writeFile(linuxPath, "linux-package");
  await writeFile(windowsPath, "windows-package");
  try {
    let started = 0;
    let releasePublishers;
    const publishersStarted = new Promise((resolve) => { releasePublishers = resolve; });
    await reconcileRegistry({
      registry: "open-vsx",
      version: "0.1.0-beta.1",
      packages: [
        { target: "linux-x64", path: linuxPath },
        { target: "win32-x64", path: windowsPath },
      ],
      async fetchImpl() { return new Response("missing", { status: 404 }); },
      async publish() {
        started += 1;
        if (started === 2) releasePublishers();
        await Promise.race([
          publishersStarted,
          new Promise((_, reject) => setTimeout(() => reject(new Error("publishers ran serially")), 100)),
        ]);
      },
    });
    assert.equal(started, 2);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("requires all canonical platform filenames", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "openaide-release-assets-"));
  try {
    await writeFile(path.join(root, "openaide-vscode-linux-x64-0.0.2.vsix"), "linux");
    await assert.rejects(collectReleasePackages(root, "0.0.2"), /win32-x64/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("collects all platform packages", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "openaide-release-assets-"));
  try {
    for (const target of ["linux-x64", "win32-x64", "darwin-arm64"]) {
      await writeFile(path.join(root, `openaide-vscode-${target}-0.0.2.vsix`), target);
    }
    const packages = await collectReleasePackages(root, "0.0.2");
    assert.deepEqual(packages.map(({ extensionId, target }) => ({ extensionId, target })), [
      { extensionId: "openaide.openaide-vscode-extension", target: "linux-x64" },
      { extensionId: "openaide.openaide-vscode-extension", target: "win32-x64" },
      { extensionId: "openaide.openaide-vscode-extension", target: "darwin-arm64" },
    ]);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

function marketplaceResponse(digest) {
  return Response.json({ results: [{ extensions: [{
    publisher: { publisherName: "openaide" },
    extensionName: "openaide-vscode-extension",
    versions: digest ? [{
      version: "0.0.2",
      targetPlatform: "linux-x64",
      properties: [{ key: "Microsoft.VisualStudio.Services.VsixSha256", value: digest }],
    }] : [],
  }] }] });
}
