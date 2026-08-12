import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import {
  formatExtensionChangelogEntry,
  prependExtensionChangelogEntry,
  updateExtensionChangelog,
} from "./update-extension-changelog.mjs";

test("leaves the extension changelog unchanged for prereleases", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "openaide-prerelease-changelog-"));
  const notesPath = path.join(root, "release-notes.md");
  const changelogPath = path.join(root, "CHANGELOG.md");
  const changelog = "# Changelog\n\n## 0.0.2 - 2026-08-09\n\nPrevious notes.\n";
  try {
    await writeFile(notesPath, "## Features\n\n- Alpha feature.\n");
    await writeFile(changelogPath, changelog);

    const updated = await updateExtensionChangelog({
      version: "0.1.0-alpha.1",
      notesPath,
      changelogPath,
      releaseDate: "2026-08-11",
    });

    assert.equal(updated, false);
    assert.equal(await readFile(changelogPath, "utf8"), changelog);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("formats release notes as a versioned extension changelog entry", () => {
  const entry = formatExtensionChangelogEntry({
    version: "0.0.3",
    releaseDate: "2026-08-10",
    releaseNotes: "## Features\r\n\r\n- Add a feature.\r\n\r\n### Detail\r\n\r\nMore context.\r\n",
  });

  assert.equal(
    entry,
    "## 0.0.3 - 2026-08-10\n\n### Features\n\n- Add a feature.\n\n#### Detail\n\nMore context.",
  );
});

test("prepends the new release without discarding history", () => {
  const updated = prependExtensionChangelogEntry({
    changelog: "# Changelog\n\n## 0.0.2 - 2026-08-09\n\nPrevious notes.\n",
    entry: "## 0.0.3 - 2026-08-10\n\nNew notes.",
    version: "0.0.3",
  });

  assert.equal(
    updated,
    "# Changelog\n\n## 0.0.3 - 2026-08-10\n\nNew notes.\n\n## 0.0.2 - 2026-08-09\n\nPrevious notes.\n",
  );
});

test("removes legacy prerelease entries when adding a stable release", () => {
  const updated = prependExtensionChangelogEntry({
    changelog: [
      "# Changelog",
      "",
      "## 0.1.0-alpha.1 - 2026-08-11",
      "",
      "Testing notes.",
      "",
      "## 0.0.2 - 2026-08-09",
      "",
      "Previous stable notes.",
      "",
    ].join("\n"),
    entry: "## 0.1.0 - 2026-08-12\n\nStable notes.",
    version: "0.1.0",
  });

  assert.equal(
    updated,
    "# Changelog\n\n## 0.1.0 - 2026-08-12\n\nStable notes.\n\n## 0.0.2 - 2026-08-09\n\nPrevious stable notes.\n",
  );
});

test("rejects duplicate versions and malformed changelog files", () => {
  const entry = "## 0.0.2 - 2026-08-09\n\nNotes.";
  assert.throws(
    () => prependExtensionChangelogEntry({ changelog: `# Changelog\n\n${entry}\n`, entry, version: "0.0.2" }),
    /already contains 0\.0\.2/,
  );
  assert.throws(
    () => prependExtensionChangelogEntry({ changelog: "## Releases\n", entry, version: "0.0.2" }),
    /must start with # Changelog/,
  );
});
