import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { parsePrivateTerms, privateDataViolations, privateTermsFile } from "./private-data-policy.mjs";

// Assembled at runtime so this file passes the check it tests.
const personalHost = ["box", "fixture", "dedyn", "io"].join(".");

function fixtureRepo(t) {
  const root = mkdtempSync(join(tmpdir(), "openaide-private-data-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  execFileSync("git", ["init", "--quiet"], { cwd: root });
  return root;
}

function stage(root, file, source) {
  mkdirSync(join(root, file, ".."), { recursive: true });
  writeFileSync(join(root, file), source);
  execFileSync("git", ["add", file], { cwd: root });
}

test("reports a staged personal hostname by location", (t) => {
  const root = fixtureRepo(t);
  stage(root, "deploy/role.env", `PORT=1\nALLOWED_HOSTS=${personalHost},localhost\n`);
  stage(root, "docs/guide.md", "Open https://example.com or http://127.0.0.1:5474.\n");

  assert.deepEqual(privateDataViolations(root), [
    { file: "deploy/role.env", line: 2, rule: "personal-host" },
  ]);
});

test("reports a locally listed private term in any letter case", (t) => {
  const root = fixtureRepo(t);
  writeFileSync(privateTermsFile(root), "# hosts\n\nPrivate-Fixture-Term\n");
  stage(root, "notes.md", "first\nsee private-fixture-term here\n");

  assert.deepEqual(privateDataViolations(root), [
    { file: "notes.md", line: 2, rule: "private-term" },
  ]);
});

test("scans staged content and leaves unstaged files alone", (t) => {
  const root = fixtureRepo(t);
  stage(root, "clean.md", "nothing private\n");
  writeFileSync(join(root, "scratch.md"), `${personalHost}\n`);

  assert.deepEqual(privateDataViolations(root), []);
});

test("reads private terms as one literal per line", () => {
  assert.deepEqual(parsePrivateTerms("# comment\n\n  a.b  \n/home/x\n"), ["a.b", "/home/x"]);
});
