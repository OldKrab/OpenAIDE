# OpenAIDE code signing policy

This policy describes which OpenAIDE release artifacts are signed, who may
authorize signing, and how the build remains traceable to public source.

## Current signing team

- **Authors:** OpenAIDE contributors whose changes are accepted into the public
  repository.
- **Reviewers:** [@OldKrab](https://github.com/OldKrab), the current repository
  maintainer and CODEOWNER. Contributions from people without commit access
  require maintainer review before merge.
- **Approvers:** [@OldKrab](https://github.com/OldKrab), the current maintainer
  authorized to approve SignPath signing requests.

The signing roles will be updated before another person receives the associated
repository or SignPath permission. Every signing participant must use multi-factor
authentication on GitHub and SignPath.

## Windows signing scope

If OpenAIDE is accepted into the SignPath Foundation program, official Windows
releases will use Authenticode signatures issued to **SignPath Foundation**.
Free code signing provided by SignPath.io, certificate by SignPath Foundation.

Only artifacts built by the protected GitHub Actions release workflow from the
tagged public OpenAIDE source may be submitted for signing. The intended signed
OpenAIDE-owned PE files are:

- `openaide-desktop.exe`, the native Desktop shell;
- `openaide-app-server.exe`, its bundled Windows App Server; and
- the final OpenAIDE NSIS installer executable.

The bundled WSL App Server is an OpenAIDE-owned Linux ELF binary and is not
Authenticode-signed. Upstream tools or libraries are not re-signed as OpenAIDE
binaries. The release workflow must inventory and verify the exact installer
payload before production signing is enabled.

The inner OpenAIDE executables are signed before they are placed into the NSIS
installer, and the completed installer is signed afterward. CI must verify the
trusted signature, timestamp, expected publisher, product name, and common
release version before publication. A signing failure blocks the signed release;
CI must not silently publish an unsigned replacement under a signed filename.

## macOS status

Current macOS bundles use Apple's free ad-hoc code signature so the bundle has
an internally consistent code seal. Ad-hoc signing does not establish an
OpenAIDE publisher identity and is not Apple notarization. Release filenames
therefore include `-unnotarized`, and users may still need to approve the first
launch in macOS Privacy & Security settings.

Developer ID signing and notarization will be documented here if they become
available. Until then, OpenAIDE does not describe macOS packages as trusted,
Developer ID-signed, or notarized.

## Release and incident controls

Release tags and GitHub release assets are immutable. Published binaries are
never replaced in place; a correction receives a new version. Signing is
restricted to approved release provenance and requires a named human approver.
The SignPath private key remains in SignPath's managed hardware and is never
stored in this repository or GitHub Actions.

The independent Desktop updater signature authenticates update bytes but does
not replace operating-system code signing. Windows in-app updates may operate
before Authenticode is available under the explicitly documented experimental
policy in [ADR-0058](docs/adr/0058-windows-updates-may-launch-before-authenticode.md).
macOS in-app updates remain disabled until their platform and installed-upgrade
gates pass.

Maintainers provision the updater Release Key and GitHub Pages with
`scripts/setup-windows-desktop-updates.sh`. The private key and its password are
GitHub Actions secrets; only the public verification key is a repository
variable. The private key must also have an encrypted backup outside GitHub.

Suspected key misuse, compromised build provenance, or an unexpected signature
must be reported privately through [SECURITY.md](SECURITY.md). Maintainers will
stop publication, withdraw affected update eligibility, preserve evidence, and
coordinate certificate revocation or key recovery as applicable.

## Installation and removal

The packages, system changes, uninstall steps, privacy behavior, and current
trust limitations are documented in the [README](README.md#install-the-desktop-app)
and [privacy policy](PRIVACY.md).
