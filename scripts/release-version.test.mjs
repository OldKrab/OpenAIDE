import assert from "node:assert/strict";
import test from "node:test";
import {
  selectPreviousReleaseTag,
  validateNextReleaseVersion,
  validateProjectVersion,
  validateReleaseTag,
} from "./release-version.mjs";

test("stable release notes compare against the previous stable tag", () => {
  assert.equal(
    selectPreviousReleaseTag({
      candidate: "0.1.0",
      tags: ["v0.0.2", "v0.1.0-alpha.1", "v0.1.0-beta.1"],
    }),
    "v0.0.2",
  );
});

test("prerelease notes compare against the immediately preceding release tag", () => {
  assert.equal(
    selectPreviousReleaseTag({
      candidate: "0.1.0-beta.2",
      tags: ["v0.0.2", "v0.1.0-alpha.1", "v0.1.0-beta.1"],
    }),
    "v0.1.0-beta.1",
  );
});

test("accepts the supported stable and numbered prerelease forms", () => {
  for (const version of ["0.0.2-alpha.1", "0.0.2-beta.12", "0.0.2-rc.1", "0.0.2"]) {
    assert.equal(validateProjectVersion(version), version);
  }
});

test("rejects versions outside the OpenAIDE release scheme", () => {
  for (const version of ["v0.0.2", "0.0.2-alpha.0", "0.0.2-preview.1", "0.0.2-alpha.01", "0.0.2+build.1"]) {
    assert.throws(() => validateProjectVersion(version), /must be X\.Y\.Z/);
  }
});

test("requires the next version to exceed the canonical version and all release tags", () => {
  const facts = { current: "0.0.1", tags: ["v0.0.1", "v0.0.2-alpha.2"] };
  assert.equal(validateNextReleaseVersion({ candidate: "0.0.2-beta.1", ...facts }), "0.0.2-beta.1");
  assert.throws(
    () => validateNextReleaseVersion({ candidate: "0.0.2-alpha.1", ...facts }),
    /must be greater than 0\.0\.2-alpha\.2/,
  );
});

test("requires release tags to match the canonical package version", () => {
  assert.equal(validateReleaseTag({ tag: "v0.0.2-rc.1", packageVersion: "0.0.2-rc.1" }), "0.0.2-rc.1");
  assert.throws(
    () => validateReleaseTag({ tag: "v0.0.2", packageVersion: "0.0.1" }),
    /does not match/,
  );
});
