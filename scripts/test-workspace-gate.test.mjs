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

test("default gate runs maintained repository integration tests", () => {
  const gate = rootPackage.scripts["test:gate"];
  for (const testFile of [
    "deploy/local-web.test.mjs",
    "scripts/local-web.test.mjs",
    "scripts/redeploy-web-dev.test.mjs",
  ]) {
    assert.match(gate, new RegExp(testFile.replaceAll(".", "\\.")));
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

test("the root package is the only source of the release version", () => {
  assert.notEqual(rootPackage.version, "0.0.0");

  for (const workspacePath of rootPackage.workspaces) {
    const workspace = packageJson(`${workspacePath}/package.json`);
    assert.equal(workspace.version, "0.0.0", `${workspace.name} must not duplicate the release version`);

    for (const dependencies of [workspace.dependencies, workspace.devDependencies]) {
      for (const [name, version] of Object.entries(dependencies ?? {})) {
        if (name.startsWith("@openaide/")) {
          assert.equal(version, "*", `${workspace.name} must link local workspace ${name} without a release pin`);
        }
      }
    }
  }

  for (const relativePath of [
    "openaide-rs/app-server/Cargo.toml",
    "openaide-rs/app-server-protocol/Cargo.toml",
  ]) {
    const manifest = readFileSync(path.join(repoRoot, relativePath), "utf8");
    assert.match(manifest, /^version = "0\.0\.0"$/m, `${relativePath} must not duplicate the release version`);
  }
});

test("a manual workflow commits and tags an exact release version", () => {
  const versionBump = readFileSync(path.join(repoRoot, ".github/workflows/version-bump.yml"), "utf8");

  assert.match(versionBump, /workflow_dispatch:/);
  assert.match(versionBump, /version:/);
  assert.match(versionBump, /release_notes:/);
  assert.match(versionBump, /type: string/);
  assert.match(versionBump, /actions\/create-github-app-token@v3/);
  assert.match(versionBump, /RELEASE_APP_ID/);
  assert.match(versionBump, /RELEASE_APP_PRIVATE_KEY/);
  assert.match(versionBump, /npm version "\$RELEASE_VERSION".*--no-git-tag-version/);
  assert.match(versionBump, /git commit --file "\$notes_path"/);
  assert.match(versionBump, /git tag --annotate "v\$RELEASE_VERSION"/);
  assert.match(versionBump, /## Changelog/);
  assert.match(versionBump, /git push --follow-tags origin main/);
  assert.doesNotMatch(versionBump, /inputs\.bump|options:\s*\n\s*- patch/);
});

test("release publishing produces every supported platform VSIX package", () => {
  const release = readFileSync(path.join(repoRoot, ".github/workflows/release.yml"), "utf8");
  const artifactBuild = readFileSync(path.join(repoRoot, ".github/workflows/build-vsix.yml"), "utf8");
  const extensionPackage = packageJson("apps/vscode-extension/package.json");

  assert.match(release, /uses: \.\/\.github\/workflows\/build-vsix\.yml/);
  assert.match(release, /version: \$\{\{ needs\.validate\.outputs\.version \}\}/);
  assert.match(artifactBuild, /target: linux-x64/);
  assert.match(artifactBuild, /target: win32-x64/);
  assert.match(artifactBuild, /target: darwin-arm64/);
  assert.match(artifactBuild, /cp LICENSE apps\/vscode-extension\/LICENSE/);
  assert.match(artifactBuild, /cd apps\/vscode-extension/);
  assert.match(artifactBuild, /@vscode\/vsce@3\.6\.0 package/);
  assert.match(artifactBuild, /--no-dependencies/);
  assert.match(artifactBuild, /node scripts\/set-release-artifact-version\.mjs "\$version"/);
  assert.doesNotMatch(artifactBuild, /extension_version=|--cwd/);
  assert.match(extensionPackage.scripts.build, /esbuild/);
  assert.match(extensionPackage.scripts.build, /--external:vscode/);
  assert.match(release, /@vscode\/vsce@3\.6\.0 publish/);
  assert.match(release, /Read release notes from version commit/);
  assert.match(release, /body_path: \$\{\{ steps\.release-notes\.outputs\.path \}\}/);
  assert.doesNotMatch(release, /generate_release_notes: true/);
  assert.match(release, /if: \$\{\{ !contains\(github\.ref_name, '-'\) \}\}/);
  assert.match(release, /VSCE_PAT: \$\{\{ secrets\.VSCE_PAT \}\}/);
  assert.doesNotMatch(release, /openaide-web-assets|docker\/build-push-action|openaide-app-server-linux/);
});

test("manual VSIX builds upload every platform without publishing a release", () => {
  const workflow = readFileSync(path.join(repoRoot, ".github/workflows/build-vsix.yml"), "utf8");

  assert.match(workflow, /workflow_dispatch:/);
  assert.match(workflow, /workflow_call:/);
  assert.match(workflow, /target: linux-x64/);
  assert.match(workflow, /target: win32-x64/);
  assert.match(workflow, /target: darwin-arm64/);
  assert.match(workflow, /actions\/upload-artifact@v7/);
  assert.match(workflow, /@vscode\/vsce@3\.6\.0 package/);
  assert.doesNotMatch(workflow, /@vscode\/vsce@3\.6\.0 publish/);
  assert.doesNotMatch(workflow, /action-gh-release|contents: write|push:\s*\n\s*tags:/);
});

function packageJson(relativePath) {
  return JSON.parse(readFileSync(path.join(repoRoot, relativePath), "utf8"));
}

function occurrences(value, search) {
  return value.split(search).length - 1;
}
