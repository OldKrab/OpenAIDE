import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import { buildDesktopUpdateFeed, writeDesktopUpdateFeed } from "./desktop-update-feed.mjs";

test("generates source-version manifests only after both platforms pass", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "openaide-update-feed-"));
  fs.writeFileSync(path.join(root, "package.json"), "{}\n");
  const feed = buildDesktopUpdateFeed(graph(), { repositoryRoot: root });
  const output = path.join(root, "feed");
  writeDesktopUpdateFeed(feed, output);

  const windows = JSON.parse(fs.readFileSync(
    path.join(output, "v1/stable/windows/x86_64/1.0.0.json"),
    "utf8",
  ));
  const mac = JSON.parse(fs.readFileSync(
    path.join(output, "v1/stable/darwin/aarch64/1.0.0.json"),
    "utf8",
  ));
  assert.equal(windows.version, "1.1.0");
  assert.equal(mac.version, "1.1.0");
  assert.equal(windows.signingIdentity, "release");
  assert.equal(windows.artifactSize, 100);
  fs.rmSync(root, { force: true, recursive: true });
});

test("rejects a one-platform feed advance", () => {
  const fixture = graph();
  fixture.edges.pop();
  assert.throws(() => buildDesktopUpdateFeed(fixture), /advance Windows and macOS together/);
});

test("rejects downgrade and mutable artifact edges", () => {
  const downgrade = graph();
  downgrade.edges[0].target = "0.9.0";
  assert.throws(() => buildDesktopUpdateFeed(downgrade), /increase SemVer/);

  const mutable = graph();
  mutable.edges[0].artifact.url = "https://github.com/OldKrab/OpenAIDE/releases/latest/download/app.exe";
  assert.throws(() => buildDesktopUpdateFeed(mutable), /immutable canonical/);
});

function graph() {
  const artifact = (platform) => ({
    url: `https://github.com/OldKrab/OpenAIDE/releases/download/v1.1.0/OpenAIDE-${platform}`,
    sha256: "a".repeat(64),
    signature: "signed:" + "b".repeat(32),
    size: 100,
  });
  return {
    schemaVersion: 1,
    repository: "OldKrab/OpenAIDE",
    edges: ["windows-x86_64", "darwin-aarch64"].map((platform) => ({
      source: "1.0.0",
      target: "1.1.0",
      channel: "stable",
      platform,
      signingIdentity: "release",
      compatibilityEvidence: "package.json",
      notes: "A compatible update.",
      publishedAt: "2026-08-28T00:00:00Z",
      artifact: artifact(platform),
    })),
  };
}
