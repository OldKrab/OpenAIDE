import assert from "node:assert/strict";
import test from "node:test";

import { buildWindowsDesktopUpdateFeed } from "./windows-desktop-update-feed.mjs";

test("routes every update-capable stable Windows release to the newest release", () => {
  const feed = buildWindowsDesktopUpdateFeed({
    releases: [release("1.1.0"), release("1.0.0"), release("0.9.0", { updater: false })],
    targetVersion: "1.1.0",
    artifactBytes: Buffer.from("signed updater bytes"),
    artifactSignature: `trusted:${"a".repeat(64)}`,
    notes: "A useful update.",
  });

  assert.deepEqual([...feed.keys()], [
    "v1/stable/windows/x86_64/1.1.0.json",
    "v1/stable/windows/x86_64/1.0.0.json",
  ]);
  const manifest = feed.get("v1/stable/windows/x86_64/1.0.0.json");
  assert.equal(manifest.version, "1.1.0");
  assert.equal(manifest.signingIdentity, "release");
  assert.equal(manifest.artifactSize, 20);
  assert.match(manifest.artifactSha256, /^[a-f0-9]{64}$/);
});

test("keeps stable and prerelease feeds separate", () => {
  const feed = buildWindowsDesktopUpdateFeed({
    releases: [release("1.1.0-beta.2", { prerelease: true }), release("1.0.0")],
    targetVersion: "1.1.0-beta.2",
    artifactBytes: Buffer.from("preview"),
    artifactSignature: "b".repeat(64),
    notes: "Preview update.",
  });

  assert.deepEqual([...feed.keys()], [
    "v1/prerelease/windows/x86_64/1.1.0-beta.2.json",
  ]);
});

test("rejects a target without its immutable updater asset", () => {
  assert.throws(
    () => buildWindowsDesktopUpdateFeed({
      releases: [release("1.1.0", { updater: false })],
      targetVersion: "1.1.0",
      artifactBytes: Buffer.from("update"),
      artifactSignature: "c".repeat(64),
      notes: "Update.",
    }),
    /does not contain/,
  );
});

function release(version, { prerelease = false, updater = true } = {}) {
  const name = `openaide-desktop-windows-x64-${version}.exe`;
  return {
    tag_name: `v${version}`,
    draft: false,
    prerelease,
    published_at: "2026-08-31T12:00:00Z",
    assets: updater ? [
      {
        name,
        browser_download_url: `https://github.com/OldKrab/OpenAIDE/releases/download/v${version}/${name}`,
      },
      { name: `${name}.sig` },
    ] : [],
  };
}
