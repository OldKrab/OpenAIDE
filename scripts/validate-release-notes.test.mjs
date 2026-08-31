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
    }),
    /duplicate the existing 0\.0\.2 changelog entry/,
  );
});

test("rejects generated notes without a Markdown section and change bullet", () => {
  assert.throws(
    () => assertReleaseNotes({
      candidate: "0.3.0",
      notes: "No user-facing changes.",
      changelog: existingChangelog,
    }),
    /must contain Markdown sections and bullet-point user impact/,
  );
});
