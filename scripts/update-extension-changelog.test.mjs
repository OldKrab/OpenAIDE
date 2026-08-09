import assert from "node:assert/strict";
import test from "node:test";
import {
  formatExtensionChangelogEntry,
  prependExtensionChangelogEntry,
} from "./update-extension-changelog.mjs";

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
