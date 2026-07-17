# Release policy

OpenAIDE uses Semantic Versioning. Until version 1.0, minor releases may contain
documented breaking changes and patch releases remain backward-compatible bug and
security fixes. Alpha, beta, and release-candidate builds use SemVer prerelease
identifiers such as `0.0.1-alpha.1`, `0.0.1-beta.1`, and `0.0.1-rc.1`.

Prereleases are testing builds. They may contain incomplete behavior, change APIs
or storage without migration support, and must not be presented as stable or
promoted to production.

## Merge requirements

- Product changes reach `main` through reviewed pull requests. The automated
  release-version commit is the only direct-push exception.
- The `TypeScript and protocol checks`, `Rust format, lint, and tests`,
  `JavaScript and TypeScript tests`, and `Production build` checks are required.
- Generated App Server Protocol bindings must be committed and current.
- Prefer squash merging so each merged pull request is one releasable change.
- Do not put credentials in repository or workflow files. Use GitHub environments
  and repository or organization secrets.

The `main` ruleset must require the repository's CI checks for pull requests and
allow the release GitHub App to push the automated version commit and tag. The
`Version Bump` workflow requires `RELEASE_APP_ID` and
`RELEASE_APP_PRIVATE_KEY`. Stable Marketplace releases also require `VSCE_PAT`.

## Creating a release

1. Confirm that `main` contains exactly the changes to release and that its CI is
   green. Choose a new exact SemVer without a `v` prefix, such as
   `0.0.1-alpha.10` or `0.0.1`.
2. In GitHub Actions, run the `Version Bump` workflow on `main` and enter that
   version. Do not edit package manifests or create the release tag manually.
   The same workflow can be dispatched with GitHub CLI:

   ```sh
   gh workflow run version-bump.yml --ref main -f version=0.0.1-alpha.10
   ```

3. The workflow validates the version, runs `npm version` to update the canonical
   root version and lockfile, creates the `Release vVERSION` commit and
   `vVERSION` tag, then pushes both to `main`.
4. The tag starts the `Release` workflow. It repeats the release checks, stamps
   the exact version into packaged manifests, builds Linux x64, Windows x64, and
   macOS Apple Silicon VSIX packages, and creates the GitHub Release. Prerelease
   versions create GitHub prereleases; stable versions also publish all platform
   packages to the VS Code Marketplace.
5. Confirm that the Release workflow completed successfully, then install and
   smoke-test each published VSIX before promoting the release.

The root `package.json` is the release-version source of truth. Package manifests
that are stamped only during artifact creation stay at their neutral source
version and must not be updated by hand.

Releases are never rebuilt in place. Correct a bad release with a new patch
or prerelease version. For example, replace a bad `0.0.1-alpha.1` build with
`0.0.1-alpha.2`. Marketplace releases are immutable; correct them with a new
version rather than rebuilding an existing tag.
