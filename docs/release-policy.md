# Release policy

OpenAIDE uses Semantic Versioning. Until version 1.0, minor releases may contain
documented breaking changes and patch releases remain backward-compatible bug and
security fixes.

## Merge requirements

- Changes reach `main` through reviewed pull requests.
- The `TypeScript and protocol checks`, `Rust format, lint, and tests`,
  `JavaScript and TypeScript tests`, and `Production build` checks are required.
- Generated App Server Protocol bindings must be committed and current.
- Prefer squash merging so each merged pull request is one releasable change.
- Do not put credentials in repository or workflow files. Use GitHub environments
  and repository or organization secrets.

Configure the `main` ruleset in GitHub after the first CI run, when the required
check names become selectable.

## Creating a release

1. Update every public package version represented by the release and commit the
   change through a pull request. The root `package.json` version is authoritative
   for the release tag.
2. Create and push an annotated `vMAJOR.MINOR.PATCH` tag from the merged `main`
   commit.
3. The release workflow validates the version, repeats tests, builds release-mode
   artifacts, publishes an immutable container, and creates a GitHub Release.
4. Verify checksums and smoke-test the published artifact before promotion.

Releases are never rebuilt in place. Correct a bad release with a new patch
version. Roll back a deployment by promoting the previously verified container
digest.

## Deployment environments

Create `staging` and `production` GitHub environments when deployment targets are
known. Production must require manual approval. Deployment workflows must consume
the published container digest from the release workflow rather than rebuild the
source. This repository intentionally does not include a provider-specific deploy
step until the target infrastructure is defined.
