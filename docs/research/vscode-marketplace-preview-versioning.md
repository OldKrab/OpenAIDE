# VS Code Marketplace preview versioning

## Question

Can OpenAIDE publish `0.0.2-alpha.1` to the VS Code Marketplace Preview
channel and later publish stable `0.0.2` under the same extension identity?

## Conclusion

No supported Marketplace workflow preserves those two semantic versions under
one extension identity. The Marketplace accepts only `major.minor.patch` for
extension versions and records Preview as version metadata. A Preview `0.0.2`
therefore consumes numeric version `0.0.2`; a regular package must use a
different version.

This is an acknowledged Marketplace limitation rather than a SemVer rule.
OpenAIDE should not bypass it through undocumented API calls or unsupported
version syntax.

## Evidence

The official publishing guide states all of the relevant constraints directly:

- extension prereleases use `vsce package --pre-release` and
  `vsce publish --pre-release`;
- Marketplace extension versions support only `major.minor.patch`, not SemVer
  prerelease identifiers;
- Preview and regular packages must have different versions;
- Microsoft recommends odd minor versions for prereleases and even minor
  versions for releases.

Source: [VS Code: Publishing Extensions — Pre-release extensions](https://code.visualstudio.com/api/working-with-extensions/publishing-extension#pre-release-extensions).

The current `vscode-vsce` publisher independently enforces the same model. It
rejects a manifest when `semver.prerelease(manifest.version)` is present. Before
upload, it also treats an existing `(version, targetPlatform)` pair as a
duplicate without considering whether either package has the Preview property;
the Marketplace API reports a conflict for the same pair.

Source: [microsoft/vscode-vsce `src/publish.ts`](https://github.com/microsoft/vscode-vsce/blob/main/src/publish.ts).

Deleting or unpublishing is not a promotion path. Microsoft documents that a
deleted extension version cannot be reused and that the latest version cannot
be deleted. Unpublishing removes availability for the extension rather than
freeing one version.

Source: [VS Code: Publishing Extensions — Removing specific extension versions](https://code.visualstudio.com/api/working-with-extensions/publishing-extension#removing-specific-extension-versions).

The manifest's separate `preview: true` field only flags the whole extension
listing as Preview. It is distinct from the opt-in prerelease update channel
and does not make a published numeric version promotable.

Source: [VS Code: Extension Manifest — `preview`](https://code.visualstudio.com/api/references/extension-manifest#fields).

Microsoft maintainers have tracked full SemVer support as a Marketplace
infrastructure gap. Microsoft extension maintainers describe odd/even numeric
tracks as their production workaround; the issue remained unresolved in 2026.

Source: [microsoft/vsmarketplace issue #50](https://github.com/microsoft/vsmarketplace/issues/50).

## Unsupported loopholes

- Manual Marketplace upload is subject to the same service model.
- Direct API upload does not change version uniqueness and would bypass the
  supported publisher safeguards.
- SemVer build metadata, four-part versions, or other syntax contradict the
  documented `major.minor.patch` contract. Passing a client-side parser is not
  evidence that the Marketplace stores or orders such a version safely.
- Deleting and republishing cannot recover a consumed version number.

## Viable release models

1. Keep one extension and use independent Marketplace numbering, such as
   `0.1.x` for Preview and `0.2.x` for regular releases. This preserves VS
   Code's normal **Switch to Pre-Release Version** experience but the
   Marketplace number differs from the OpenAIDE product version.
2. Publish a separate Preview extension identity. The stable extension can keep
   product-aligned numeric versions, but users do not get the normal same-listing
   channel switch and migration requires extra coordination.
3. Keep SemVer prereleases on GitHub as installable VSIX assets and publish only
   stable product versions to the Marketplace.

If exact SemVer identity and the same Marketplace listing are both hard
requirements, the remaining path is to request support from the Marketplace
team and wait for an officially supported capability; there is no safe local
workflow change that supplies it.
