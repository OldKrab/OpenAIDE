import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const rootPackage = packageJson("package.json");
const maintainedWorkspaceNames = [
  "@openaide/app-server-client",
  "@openaide/app-shell-contracts",
  "openaide-frontend",
  "openaide-vscode-extension",
  "openaide-web",
];

test("default npm test runs the backend and every maintained workspace exactly once", () => {
  const rootTest = rootPackage.scripts.test;
  assert.equal(occurrences(rootTest, "npm run backend:test"), 1);
  assert.equal(occurrences(rootTest, "npm run test --workspaces --if-present"), 1);
  assert.doesNotMatch(rootTest, /--workspace(?:=|\s)/);

  const workspacePackages = rootPackage.workspaces.map((workspace) => packageJson(`${workspace}/package.json`));
  assert.deepEqual(
    workspacePackages.map((workspace) => workspace.name).sort(),
    maintainedWorkspaceNames.toSorted(),
  );
  for (const workspace of workspacePackages) {
    assert.ok(workspace.scripts?.test, `${workspace.name} must define a test script`);
  }
});

test("default npm check includes every workspace that exposes a check script", () => {
  const rootCheck = rootPackage.scripts.check;
  assert.equal(occurrences(rootCheck, "npm run check --workspaces --if-present"), 1);

  const workspacePackages = rootPackage.workspaces.map((workspace) => packageJson(`${workspace}/package.json`));
  const checkedWorkspaceNames = workspacePackages
    .filter((workspace) => workspace.scripts?.check)
    .map((workspace) => workspace.name)
    .sort();
  assert.deepEqual(checkedWorkspaceNames, [
    "@openaide/app-server-client",
    "@openaide/app-shell-contracts",
    "openaide-frontend",
    "openaide-vscode-extension",
  ]);
});

function packageJson(relativePath) {
  return JSON.parse(readFileSync(path.join(repoRoot, relativePath), "utf8"));
}

function occurrences(value, search) {
  return value.split(search).length - 1;
}
