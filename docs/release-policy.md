# Release policy

OpenAIDE uses Semantic Versioning. Until version 1.0, minor releases may contain
documented breaking changes and patch releases remain backward-compatible bug and
security fixes. Alpha, beta, and release-candidate builds use SemVer prerelease
identifiers such as `0.0.1-alpha.1`, `0.0.1-beta.1`, and `0.0.1-rc.1`.

Prereleases are testing builds. They may contain incomplete behavior, change APIs
or storage without migration support, and must not be presented as stable or
promoted to production.

## Merge requirements

- Changes reach `master` through reviewed pull requests.
- The `TypeScript and protocol checks`, `Rust format, lint, and tests`,
  `JavaScript and TypeScript tests`, and `Production build` checks are required.
- Generated App Server Protocol bindings must be committed and current.
- Prefer squash merging so each merged pull request is one releasable change.
- Do not put credentials in repository or workflow files. Use GitHub environments
  and repository or organization secrets.

Configure the `master` ruleset in GitHub after the first CI run, when the required
check names become selectable.

## Creating a release

1. Update every public package version represented by the release and commit the
   change through a pull request. The root `package.json` version is authoritative
   for the release tag.
2. Create and push an annotated SemVer tag from the merged `master` commit, such as
   `v0.0.1-alpha.1` for a prerelease or `v0.0.1` for a stable release.
3. The release workflow validates the version, repeats tests, builds release-mode
   artifacts, publishes a versioned container, and creates a GitHub Release.
   Tags with a prerelease identifier create a GitHub prerelease and only publish
   the exact prerelease container tag. Stable releases may additionally update the
   matching `MAJOR.MINOR` container tag.
4. Verify checksums and smoke-test the published artifact before promotion.

Releases are never rebuilt in place. Correct a bad release with a new patch
or prerelease version. For example, replace a bad `0.0.1-alpha.1` build with
`0.0.1-alpha.2`. Roll back a deployment by promoting the previously verified
container digest.

## Deployment environments

Create `staging` and `production` GitHub environments when deployment targets are
known. Production must require manual approval. Deployment workflows must consume
the published container digest from the release workflow rather than rebuild the
source. This repository intentionally does not include a provider-specific deploy
step until the target infrastructure is defined.
