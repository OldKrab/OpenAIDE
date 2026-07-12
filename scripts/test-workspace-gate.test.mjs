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
  assert.equal(occurrences(rootTest, "npm run build:typescript-deps"), 1);
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
  assert.equal(occurrences(rootCheck, "npm run build:typescript-deps"), 1);
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

test("the local CI command runs every required validation class", () => {
  const ci = rootPackage.scripts.ci;
  assert.match(ci, /cargo fmt --all --check/);
  assert.match(ci, /cargo clippy --workspace --all-targets -- -D warnings/);
  assert.match(ci, /npm run check/);
  assert.match(ci, /npm run test/);
  assert.match(ci, /npm run build/);
});

test("release publishing produces only Linux and Windows VSIX packages", () => {
  const release = readFileSync(path.join(repoRoot, ".github/workflows/release.yml"), "utf8");

  assert.match(release, /target: linux-x64/);
  assert.match(release, /target: win32-x64/);
  assert.match(release, /@vscode\/vsce@3\.6\.0 package/);
  assert.match(release, /@vscode\/vsce@3\.6\.0 publish/);
  assert.match(release, /if: \$\{\{ !contains\(github\.ref_name, '-'\) \}\}/);
  assert.match(release, /VSCE_PAT: \$\{\{ secrets\.VSCE_PAT \}\}/);
  assert.doesNotMatch(release, /openaide-web-assets|docker\/build-push-action|openaide-app-server-linux/);
});

function packageJson(relativePath) {
  return JSON.parse(readFileSync(path.join(repoRoot, relativePath), "utf8"));
}

function occurrences(value, search) {
  return value.split(search).length - 1;
}
