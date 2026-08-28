import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { resolveReleaseArtifactMatrices } from "./resolve-release-artifact-matrices.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const rootPackage = packageJson("package.json");
const maintainedWorkspaceNames = [
  "@openaide/app-server-client",
  "@openaide/app-shell-contracts",
  "openaide-desktop",
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
    "scripts/check-release-main-ci.test.mjs",
    "scripts/local-web.test.mjs",
    "scripts/reconcile-release-registry.test.mjs",
    "scripts/redeploy-web-dev.test.mjs",
    "scripts/release-version.test.mjs",
    "scripts/smoke-release-vsix.test.mjs",
    "scripts/validate-release-notes.test.mjs",
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
    "openaide-desktop",
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

test("workflows pin every external action and declared release toolchain", () => {
  const workflowDirectory = path.join(repoRoot, ".github", "workflows");
  for (const workflowName of ["build-vsix.yml", "ci.yml", "reconcile-release.yml", "release.yml", "version-bump.yml"]) {
    const workflow = readFileSync(path.join(workflowDirectory, workflowName), "utf8");
    for (const match of workflow.matchAll(/^\s*-?\s*uses:\s+([^\s#]+)/gm)) {
      if (!match[1].startsWith("./")) {
        assert.match(match[1], /@[0-9a-f]{40}$/, `${workflowName} must pin ${match[1]} to a commit`);
      }
    }
  }

  assert.equal(readFileSync(path.join(repoRoot, ".node-version"), "utf8").trim(), "24.18.0");
  assert.match(readFileSync(path.join(repoRoot, "rust-toolchain.toml"), "utf8"), /channel = "1\.97\.1"/);
  assert.match(rootPackage.devDependencies["@vscode/vsce"], /^\d+\.\d+\.\d+$/);
  assert.equal(rootPackage.devDependencies.ovsx, "1.0.2");
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
  assert.match(versionBump, /type: string/);
  assert.match(versionBump, /release_notes:[\s\S]*Generated Markdown for this release[\s\S]*required: true/);
  assert.match(versionBump, /concurrency:[\s\S]*cancel-in-progress: false/);
  assert.match(versionBump, /actions\/create-github-app-token@[0-9a-f]{40}/);
  assert.match(versionBump, /client-id: \$\{\{ secrets\.RELEASE_APP_CLIENT_ID \}\}/);
  assert.match(versionBump, /RELEASE_APP_PRIVATE_KEY/);
  assert.match(versionBump, /npm version "\$RELEASE_VERSION".*--no-git-tag-version/);
  assert.match(versionBump, /git commit --file "\$notes_path"/);
  assert.match(versionBump, /git tag --annotate "v\$RELEASE_VERSION"/);
  assert.match(versionBump, /node scripts\/check-release-main-ci\.mjs/);
  assert.match(versionBump, /node scripts\/release-version\.mjs next "\$RELEASE_VERSION"/);
  assert.match(versionBump, /RELEASE_NOTES: \$\{\{ inputs\.release_notes \}\}/);
  assert.match(versionBump, /printf '%s\\n' "\$RELEASE_NOTES" > "\$notes_path"/);
  assert.match(versionBump, /node scripts\/validate-release-notes\.mjs/);
  assert.doesNotMatch(versionBump, /releases\/generate-notes|cp release-notes\.md|git diff --quiet .*release-notes\.md/);
  assert.match(versionBump, /node scripts\/update-extension-changelog\.mjs "\$RELEASE_VERSION" "\$notes_path"/);
  assert.match(versionBump, /git add package\.json package-lock\.json apps\/vscode-extension\/CHANGELOG\.md/);
  assert.match(versionBump, /git push --atomic origin "HEAD:refs\/heads\/main" "refs\/tags\/v\$RELEASE_VERSION"/);
  assert.doesNotMatch(versionBump, /--follow-tags/);
  assert.doesNotMatch(versionBump, /inputs\.bump|options:\s*\n\s*- patch/);
});

test("release publishing produces every supported VSIX and desktop package", () => {
  const release = readFileSync(path.join(repoRoot, ".github/workflows/release.yml"), "utf8");
  const artifactBuild = readFileSync(path.join(repoRoot, ".github/workflows/build-vsix.yml"), "utf8");
  const extensionPackage = packageJson("apps/vscode-extension/package.json");

  assert.match(release, /uses: \.\/\.github\/workflows\/build-vsix\.yml/);
  assert.match(release, /artifacts: all/);
  assert.match(release, /version: \$\{\{ needs\.validate\.outputs\.version \}\}/);
  assert.match(artifactBuild, /cp LICENSE apps\/vscode-extension\/LICENSE/);
  assert.match(artifactBuild, /cd apps\/vscode-extension/);
  assert.match(artifactBuild, /npm exec -- vsce package/);
  assert.match(artifactBuild, /RELEASE_VERSION: \$\{\{ needs\.prepare\.outputs\.version \}\}/);
  assert.doesNotMatch(artifactBuild, /RELEASE_VERSION: \$\{\{ steps\.version\.outputs\.version \}\}/);
  assert.match(artifactBuild, /--no-dependencies/);
  assert.match(artifactBuild, /cargo build --locked --release/);
  // The shared App Server job has one command for target-specific builds and
  // one fallback command for hosts without a Rust target override.
  assert.equal(occurrences(artifactBuild, "cargo build --locked --release -p openaide-app-server"), 2);
  assert.match(artifactBuild, /name: app-server-\$\{\{ matrix\.target \}\}/);
  assert.match(artifactBuild, /actions\/download-artifact@[0-9a-f]{40}/);
  assert.match(artifactBuild, /needs: \[prepare, app-server\]/);
  assert.match(artifactBuild, /node scripts\/smoke-release-vsix\.mjs/);
  assert.match(artifactBuild, /node scripts\/set-release-artifact-version\.mjs "\$\{\{ needs\.prepare\.outputs\.version \}\}"/);
  assert.match(artifactBuild, /--bundles "\$\{\{ matrix\.bundle \}\}"/);
  assert.match(artifactBuild, /tauri\.release-bundle\.conf\.json/);
  assert.match(artifactBuild, /openaide-app-server-\$\{\{ matrix\.target_triple \}\}/);
  assert.match(release, /openaide-desktop-win32-x64-\$RELEASE_VERSION-unsigned\.exe/);
  assert.match(release, /openaide-desktop-darwin-arm64-\$RELEASE_VERSION-unsigned\.dmg/);
  assert.doesNotMatch(artifactBuild, /extension_version=|--cwd/);
  assert.match(extensionPackage.scripts.build, /esbuild/);
  assert.match(extensionPackage.scripts.build, /--external:vscode/);
  assert.deepEqual(extensionPackage.extensionKind, ["workspace"]);
  assert.equal(extensionPackage.extensionDependencies, undefined);
  assert.doesNotMatch(artifactBuild, /notification-companion|apps\/vscode-notification-companion/);
  assert.doesNotMatch(release, /notification-companion|apps\/vscode-notification-companion/);
  assert.match(release, /Read release notes from version commit/);
  assert.match(release, /body_path: \$\{\{ steps\.release-notes\.outputs\.path \}\}/);
  assert.match(release, /draft: true/);
  assert.match(release, /gh release edit "\$GITHUB_REF_NAME" --draft=false/);
  assert.match(release, /\.immutable/);
  assert.match(release, /git merge-base --is-ancestor/);
  assert.doesNotMatch(release, /generate_release_notes: true/);
  assert.match(release, /reconcile-release-registry\.mjs marketplace/);
  assert.match(release, /VSCE_PAT: \$\{\{ secrets\.VSCE_PAT \}\}/);
  assert.match(release, /name: Reconcile Open VSX packages/);
  assert.match(release, /reconcile-release-registry\.mjs open-vsx/);
  assert.match(release, /OVSX_PAT: \$\{\{ secrets\.OVSX_PAT \}\}/);
  assert.match(release, /if: \$\{\{ !contains\(github\.ref_name, '-'\) \}\}/);
  assert.doesNotMatch(release, /openaide-web-assets|docker\/build-push-action|openaide-app-server-linux/);
});

test("manual artifact builds can select VSIX, desktop, or all without publishing", () => {
  const workflow = readFileSync(path.join(repoRoot, ".github/workflows/build-vsix.yml"), "utf8");

  assert.match(workflow, /workflow_dispatch:/);
  assert.match(workflow, /workflow_call:/);
  assert.match(workflow, /options: \[all, vsix, desktop\]/);
  assert.match(workflow, /vsix_target:/);
  assert.match(workflow, /options: \[all, linux-x64, win32-x64, darwin-arm64\]/);
  assert.match(workflow, /vsix_matrix: \$\{\{ steps\.build\.outputs\.vsix_matrix \}\}/);
  assert.match(workflow, /matrix: \$\{\{ fromJSON\(needs\.prepare\.outputs\.vsix_matrix\) \}\}/);
  assert.match(workflow, /actions\/upload-artifact@[0-9a-f]{40}/);
  assert.match(workflow, /npm exec -- vsce package/);
  assert.match(workflow, /short_sha="\$\{GITHUB_SHA:0:7\}"/);
  assert.match(workflow, /version="\$\{base_version\}\.g\$\{short_sha\}"/);
  assert.match(workflow, /version="\$\{base_version\}-g\$\{short_sha\}"/);
  assert.match(workflow, /name: vsix-\$\{\{ matrix\.target \}\}/);
  assert.match(workflow, /name: desktop-\$\{\{ matrix\.target \}\}/);
  assert.doesNotMatch(workflow, /vsce publish/);
  assert.doesNotMatch(workflow, /action-gh-release|contents: write|push:\s*\n\s*tags:/);
});

test("artifact builds select only the requested package targets and required servers", () => {
  const windows = resolveReleaseArtifactMatrices({
    artifacts: "all",
    vsixTarget: "win32-x64",
    desktopTarget: "win32-x64",
  });
  assert.deepEqual(windows.vsix_matrix.include.map(({ target }) => target), ["win32-x64"]);
  assert.deepEqual(windows.desktop_matrix.include.map(({ target }) => target), ["win32-x64"]);
  assert.deepEqual(windows.server_matrix.include.map(({ target }) => target), ["win32-x64", "linux-x64-musl"]);

  const fullRelease = resolveReleaseArtifactMatrices({
    artifacts: "all",
    vsixTarget: "all",
    desktopTarget: "all",
  });
  assert.deepEqual(fullRelease.vsix_matrix.include.map(({ target }) => target), vsixTargets());
  assert.deepEqual(fullRelease.desktop_matrix.include.map(({ target }) => target), ["win32-x64", "darwin-arm64"]);
  assert.deepEqual(fullRelease.server_matrix.include.map(({ target }) => target), [
    ...vsixTargets(),
    "linux-x64-musl",
  ]);
});

function packageJson(relativePath) {
  return JSON.parse(readFileSync(path.join(repoRoot, relativePath), "utf8"));
}

function occurrences(value, search) {
  return value.split(search).length - 1;
}

function vsixTargets() {
  return ["linux-x64", "win32-x64", "darwin-arm64"];
}
