# Release policy

OpenAIDE release versions are deliberately narrower than general Semantic
Versioning. A version must be either `X.Y.Z` or
`X.Y.Z-(alpha|beta|rc).N`, where `N` starts at 1. Every new version must be
greater than the current package version and every existing release tag.

Until version 1.0, minor releases may contain documented breaking changes and
patch releases remain backward-compatible bug and security fixes. Prereleases
are testing builds. They may contain incomplete behavior, change APIs or storage
without migration support, and must not be presented as stable.

## Repository requirements

- Product changes reach `main` through reviewed pull requests. The automated
  release-version commit is the only direct-push exception.
- Require `TypeScript and protocol checks`, `Rust format, lint, and tests`,
  `JavaScript and TypeScript tests`, `Task Chat smoke tests`, and
  `Production build` on pull requests.
- Allow the release GitHub App to push the automated version commit and tag.
- Enable **immutable releases** in repository settings. The release workflow
  stops before registry publication if GitHub does not report the canonical
  release as immutable.
- Configure `RELEASE_APP_CLIENT_ID` and `RELEASE_APP_PRIVATE_KEY`. Stable
  registry publication also requires `VSCE_PAT` and `OVSX_PAT` as repository or
  organization secrets. `RELEASE_APP_ID` is obsolete and should be removed.
- Generated App Server Protocol bindings must be committed and current.

The workflows pin Node, Rust, publishing tools, and third-party actions. Cargo
release builds use the committed lockfile. Dependency/toolchain pin updates are
normal reviewed changes, not release-time downloads of an unspecified latest
version.

## Open VSX setup

OpenAIDE publishes prereleases through Open VSX's native prerelease channel.
Stable releases are published to Open VSX and the VS Code Marketplace. Do not
backfill `0.0.1`.

Before that release:

1. Create an [Eclipse account](https://accounts.eclipse.org/user/register), sign
   in to [Open VSX](https://open-vsx.org/), connect the same GitHub and Eclipse
   accounts, and accept the Publisher Agreement from the Open VSX profile page.
2. Generate a dedicated CI token from the
   [Open VSX token settings](https://open-vsx.org/user-settings/tokens).
3. With that token available locally as `OVSX_PAT`, run
   `npm exec -- ovsx create-namespace openaide`, then
   [claim namespace ownership](https://github.com/EclipseFdn/open-vsx.org/wiki/Managing-Namespaces).
4. Run `gh secret set OVSX_PAT` and enter the token at the hidden prompt. Never
   pass it in command arguments or commit it to a file.

## Creating a release

1. Confirm that `main` contains exactly the changes to release. Choose a new
   OpenAIDE version without a `v` prefix, such as `0.0.2-beta.1` or `0.0.2`.
2. Write concise user-facing Markdown in `release-notes.md`. Prefer sections
   such as `## Features`, `## Bug Fixes`, and `## Chores`; describe user impact.
   Do not add a changelog section because the workflow appends it. These notes
   become the GitHub Release body for every release, but only stable releases
   add them to the extension changelog. Alpha, beta, and release-candidate
   versions never create extension changelog entries.
3. Run `Version Bump` on `main` in GitHub Actions, or dispatch it with:

   ```sh
   gh workflow run version-bump.yml --ref main \
     -f version=0.0.2-beta.1 \
     -F release_notes=@release-notes.md
   ```

4. `Version Bump` is serialized and cannot be cancelled in progress. It checks
   that the exact checked-out `main` commit has a completed successful CI push
   run, validates the monotonic version, updates the root package and lockfile,
   updates the extension changelog for stable releases, commits the release
   notes, creates the explicit tag, and atomically pushes the `main` update and
   tag.
5. The tag starts `Release`. It rejects tags not reachable from `main`, then
   builds Linux x64, Windows x64, and macOS Apple Silicon VSIX packages while
   normal CI validates the exact version commit. Publication requires both to
   succeed; the release workflow does not repeat the CI suite. Prerelease
   packages carry the registry's native prerelease metadata. Each runner
   inspects the packaged files and exercises its bundled App Server through
   startup and graceful JSON-RPC shutdown before the VSIX can be uploaded.
6. The workflow creates a draft GitHub Release, attaches the complete verified
   asset set, publishes the draft, and verifies immutability. This GitHub
   Release is the canonical release. Every release then reconciles the same
   downloaded bytes with Open VSX; stable releases also reconcile them with the
   VS Code Marketplace.
7. Confirm that the Release workflow completed successfully. No manual rebuild
   or replacement of its assets is permitted.

The root `package.json` is the release-version source of truth. Package and
Cargo manifests stamped only during artifact creation stay at `0.0.0` in source
and must not be updated by hand.

## Recovery and registry reconciliation

GitHub assets and registry packages are immutable release facts. A bad artifact
requires a new patch or prerelease version; for example, replace a bad
`0.0.2-beta.1` with `0.0.2-beta.2`.

If registry publication was merely interrupted, run **Reconcile Release
Registries** with the existing version. It downloads the immutable GitHub assets
and handles each applicable target independently. Prereleases reconcile only
Open VSX; stable releases reconcile both registries:

- missing package: publish it;
- same version, target, and SHA-256: skip successfully;
- same version and target with a different SHA-256: fail without publishing.

Publisher acceptance completes a missing-package attempt. The reconciler checks
all target digests before publishing, then submits missing platform packages in
parallel. It does not wait for eventually consistent registry indexing or
security scans after publication; a later run verifies the published SHA-256
and fills any target whose earlier publication was interrupted. Recovery
validates the requested release at its immutable tag, then uses the current
reviewed reconciler from `main` so publishing fixes can recover older releases
without rebuilding or changing their assets.

This recovery path resumes incomplete publication; it never rebuilds a release.
