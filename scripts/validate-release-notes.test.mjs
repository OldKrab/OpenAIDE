import assert from "node:assert/strict";
import test from "node:test";
import { assertReleaseNotes } from "./validate-release-notes.mjs";

const existingChangelog = [
  "# Changelog",
  "",
  "## 0.2.1 - 2026-08-17",
  "",
  "### Features",
  "",
  "- Add the desktop shell.",
  "",
  "## 0.0.2 - 2026-08-09",
  "",
  "### Features",
  "",
  "- Add durable message queuing.",
  "",
].join("\n");

test("accepts changed user-facing notes that are new to the changelog", () => {
  assert.equal(
    assertReleaseNotes({
      candidate: "0.3.0",
      notes: "## Features\n\n- Add a new desktop capability.\n",
      changelog: existingChangelog,
      previousTag: "v0.2.1",
      sourceChangedSincePrevious: true,
    }),
    "## Features\n\n- Add a new desktop capability.",
  );
});

test("rejects notes copied from any existing changelog entry", () => {
  assert.throws(
    () => assertReleaseNotes({
      candidate: "0.3.0",
      notes: "## Features\n\n- Add durable message queuing.\n",
      changelog: existingChangelog,
      previousTag: "v0.2.1",
      sourceChangedSincePrevious: true,
    }),
    /duplicate the existing 0\.0\.2 changelog entry/,
  );
});

test("rejects release notes when the source file was not updated since the baseline", () => {
  assert.throws(
    () => assertReleaseNotes({
      candidate: "0.3.0",
      notes: "## Features\n\n- Add a new desktop capability.\n",
      changelog: existingChangelog,
      previousTag: "v0.2.1",
      sourceChangedSincePrevious: false,
    }),
    /release-notes\.md must change after v0\.2\.1/,
  );
});

test("rejects release notes that provide their own changelog link", () => {
  assert.throws(
    () => assertReleaseNotes({
      candidate: "0.3.0",
      notes: "## Features\n\n- Add a feature.\n\n## Changelog\n\nLink.",
      changelog: existingChangelog,
      previousTag: "v0.2.1",
      sourceChangedSincePrevious: true,
    }),
    /must not contain a Changelog section/,
  );
});

test("rejects the checked-in next-release template", () => {
  assert.throws(
    () => assertReleaseNotes({
      candidate: "0.3.0",
      notes: "<!-- OPENAIDE_RELEASE_NOTES_TEMPLATE -->\n\n## Features\n\n- Replace this placeholder.",
      changelog: existingChangelog,
      previousTag: "v0.2.1",
      sourceChangedSincePrevious: true,
    }),
    /replace the release-notes\.md template/,
  );
});
