# SignPath Foundation Windows onboarding

## Decision

SignPath Foundation is a viable zero-cash route to Authenticode-sign OpenAIDE's
Windows releases if SignPath accepts the project. It is not a fully unattended
release service: every Foundation-signed release requires approval by a named
human approver.

This note was checked on 2026-08-31 against SignPath-owned documentation and its
official GitHub Action.

## Eligibility and application

The free OSS subscription requires all of the following:

- no malware or potentially unwanted software;
- an OSI-approved license, without commercial dual licensing, for every project
  component;
- no proprietary project components, except system libraries;
- an actively maintained project that is already released in the form to be
  signed;
- documented product functionality on its download page or store listing.

For a Foundation certificate, the code-signing team must also own and maintain
the repository and build scripts and may sign only binaries built from its own
source. Unsigned upstream OSS binaries may be included in a signed installer,
but must not be signed as OpenAIDE binaries. SignPath does not currently accept
hacking or security-circumvention tools. Acceptance is discretionary and
includes a project-reputation assessment.

Sources: [Foundation conditions](https://signpath.org/terms.html),
[application](https://signpath.org/apply.html).

OpenAIDE appears to satisfy the license, maintenance, and prior-release basics,
but is not ready to apply yet:

- `README.md` still says Desktop is planned, despite the published Desktop
  installer, so it does not accurately document the form to be signed. The
  public
  [`v0.4.0` release](https://github.com/OldKrab/OpenAIDE/releases/tag/v0.4.0)
  does establish that the Windows NSIS form has already been released;
- the repository has no public privacy policy;
- the repository and release/download surfaces have no required **Code signing
  policy**;
- the packaged-file inventory still needs to classify OpenAIDE-owned and
  upstream binaries so the artifact configuration signs only allowed files;
- SignPath should confirm that a general coding-agent host with terminal access
  is outside its prohibited hacking-tool category.

OpenAIDE is AGPL-3.0-only at the repository root and in the Desktop package.
That supports, but does not by itself prove, the all-components requirement:
the exact release payload and dependency licenses still need an inventory.

## Required public policy and team controls

The project must:

- enable MFA for every participating team member on GitHub and SignPath;
- name Authors, Reviewers, and Approvers;
- require team review for contributions by non-committers;
- publish a **Code signing policy** link or section on both the project homepage
  and release/download pages;
- include the exact attribution “Free code signing provided by SignPath.io,
  certificate by SignPath Foundation”;
- list the people or GitHub teams holding each signing role;
- link an accurate privacy policy, including affected third-party services;
- warn about system changes and provide an uninstall path;
- enforce `OpenAIDE` as product-name metadata and one consistent product version
  across each signed build.

If the application transfers user data to a system the user did not specify,
SignPath requires the privacy policy to be shown during installation and an
installation option to disable that transfer. OpenAIDE's automatic update check
and third-party Agent traffic therefore need an explicit, truthful privacy and
installer review before applying; the short “no unsolicited transfer” template
must not be copied if it is false.

Source: [Foundation conditions](https://signpath.org/terms.html).

## GitHub Actions flow after approval

SignPath's GitHub connector requires the artifact to be built by a GitHub
workflow, uploaded as a GitHub Actions artifact before submission, and—on the
OSS plan—produced entirely by preceding GitHub-hosted jobs. The recommended
release flow is:

1. Build the unsigned Windows artifacts on GitHub-hosted runners.
2. Upload them with `actions/upload-artifact` v4 or newer.
3. Submit that exact artifact with
   `signpath/github-action-submit-signing-request@v2`.
4. A named approver receives an email and approves or denies the release in
   SignPath.
5. The action waits, downloads the signed output into the specified directory,
   and CI verifies and publishes that output only.

Required action inputs are:

- a `SIGNPATH_API_TOKEN` GitHub Actions secret for a dedicated, least-privilege
  CI user with submitter permission;
- the SignPath organization ID;
- project and signing-policy slugs;
- the uploaded GitHub artifact ID;
- an artifact-configuration slug unless the project default is used.

The GitHub App and predefined GitHub.com trusted build system are configured in
SignPath and linked to the project. GitHub does not receive the Authenticode
private key; SignPath stores it in its HSM. The API token is displayed only when
created and must remain a CI secret.

Sources: [GitHub trusted-build integration](https://docs.signpath.io/trusted-build-systems/github),
[official action](https://github.com/SignPath/github-action-submit-signing-request),
[user and CI-token management](https://docs.signpath.io/users/),
[Foundation service](https://signpath.org/).

## OpenAIDE artifact shape

SignPath can sign several PE files placed in one ZIP. Its documented deep-signing
support covers composite formats such as MSI, CAB, MSIX, and AppX, but does not
document unpacking and repacking NSIS installers. OpenAIDE currently publishes a
Tauri NSIS `.exe`. SignPath's format reference treats a generic `.exe` as a
non-composite PE: it can sign the outer NSIS file, but it cannot thereby replace
and sign the executables stored inside it.

**Therefore the documented SignPath capabilities require two signing requests,
not one deep-signing request:**

1. Build the unpackaged Desktop executable and stage the App Server sidecar.
2. Put `openaide-desktop.exe` and `openaide-app-server.exe` in one ZIP-backed
   GitHub Actions artifact and submit request 1 with a `<zip-file>` artifact
   configuration containing exactly those two `<pe-file>` entries.
3. Replace the unsigned inputs with SignPath's signed outputs and bundle those
   files into the Tauri NSIS installer. Do not sign the Linux WSL App Server or
   any upstream executable with the Foundation certificate.
4. Upload the final NSIS installer as a new GitHub Actions artifact and submit
   request 2. Its artifact configuration signs the installer as a `<pe-file>`.
5. Verify the two installed OpenAIDE PE files and the outer installer before
   publishing. Audit the generated NSIS uninstaller and any third-party
   bootstrapper separately rather than assuming the outer signature covers
   them.

The Foundation terms say every release needs manual approval, while SignPath's
approval mechanism applies to signing requests. The public documentation does
not define a way to group these two requests into one approval. Plan for **two
human approvals per Windows release** unless SignPath explicitly configures or
documents a one-release approval mechanism during onboarding.

Sources: [artifact-configuration syntax](https://docs.signpath.io/artifact-configuration/syntax),
[examples](https://docs.signpath.io/artifact-configuration/examples),
[reference](https://docs.signpath.io/artifact-configuration/reference).

### Current workflow changes

The current reusable artifact workflow performs one Tauri `build --no-sign`,
then copies the NSIS output to a filename ending in `-unsigned.exe`.
The release workflow requires that unsigned filename before publication.
`tauri.release-bundle.conf.json` identifies `openaide-app-server` as the bundled
sidecar, and the Windows configuration also includes a Linux App Server for WSL.

After acceptance, the repository changes are:

1. Add Windows version-resource metadata to every OpenAIDE-owned PE selected for
   signing. `openaide-app-server.exe` currently has no explicit version-resource
   setup in its `build.rs`; confirm the built Desktop PE metadata too. Enforce
   product name `OpenAIDE` and one release version in SignPath's metadata
   restrictions.
2. Split the Windows Desktop job into build-without-bundle, inner-PE signing,
   NSIS bundling, and outer-installer signing. macOS and manual unsigned builds
   remain on their existing path.
3. Add two pinned uses of the official
   `signpath/github-action-submit-signing-request@v2`, each consuming the
   preceding upload step's `artifact-id` and downloading the signed result.
4. Keep `SIGNPATH_API_TOKEN` as the only SignPath secret. Store the organization
   ID as a repository variable; check the project, signing-policy, and two
   artifact-configuration slugs into the workflow because they are identifiers,
   not credentials.
5. Add release-blocking Authenticode verification for the inner executables and
   installer, including trusted chain, timestamp, expected publisher, product
   metadata, and version. Keep Tauri updater-signature verification separate.
6. Rename the signed output without `-unsigned` and change the release asset-set
   assertion to reject an unsigned Windows fallback when signed Windows release
   mode is selected.

Relevant repository evidence:
[artifact workflow](../../.github/workflows/build-vsix.yml),
[release workflow](../../.github/workflows/release.yml),
[sidecar configuration](../../apps/desktop/src-tauri/tauri.release-bundle.conf.json),
[Windows resources](../../apps/desktop/src-tauri/tauri.windows.conf.json), and
[App Server build metadata](../../openaide-rs/app-server/build.rs).

The exact Tauri split command and NSIS uninstaller behavior must be proven on a
Windows branch before merging; this note defines the signing boundary, not an
untested command sequence.

## Human setup versus repository work

| Human / service setup | Repository work |
| --- | --- |
| Enable GitHub MFA for every signing participant and prepare named Authors, Reviewers, and Approvers. | Correct Desktop documentation; add privacy and Code signing policies and expose their links on README/download/release surfaces. |
| Ask SignPath about coding-agent eligibility, NSIS two-request approvals, and the generated uninstaller; then submit the Foundation application. | Inventory the exact Windows package and dependency licenses; mark OpenAIDE-owned versus upstream executables. |
| After acceptance, accept SignPath invitations, enable SignPath MFA, install the SignPath GitHub App for this repository, and link the predefined GitHub.com trusted build system to the SignPath project. | Add PE product/version metadata and implement the two-stage signing workflow behind release-only conditions. |
| Configure a release-signing policy restricted to trusted GitHub origin and the intended release ref, with the required approver(s). Create the two artifact configurations from real samples and apply metadata restrictions. | Verify signatures and timestamps, smoke-test the installed signed build, and make signed-artifact presence mandatory for publication. |
| Create a dedicated CI user with submitter permission only, generate its API token, and save it as the `SIGNPATH_API_TOKEN` GitHub Actions secret. Record the organization ID and assigned/configured slugs as repository variables/workflow constants. | Retain the separate Tauri updater key and updater-artifact signing; Authenticode does not replace it. |

SignPath's Open Source Code Signing policy requires trusted-build-system and
origin verification. Its GitHub connector verifies that the artifact was first
stored as a GitHub Actions artifact and that every preceding OSS workflow job ran
on GitHub-hosted runners. OpenAIDE's current Windows pipeline uses GitHub-hosted
runners, so it fits that boundary; a future self-hosted job in the dependency
chain would invalidate it.

## Manual approval and end-user result

Foundation policy explicitly requires manual approval for every release.
SignPath supports an approval quorum and emails approvers about pending requests.
For a small project, the practical flow is therefore: start the release, wait for
CI to submit the request, approve it in SignPath, and let CI resume publication.

The Windows publisher will be **SignPath Foundation**, not OpenAIDE, because the
certificate is legally issued to the Foundation. A valid trusted signature
removes `Unknown publisher`, but it does not guarantee that SmartScreen will be
silent immediately: SignPath's own Windows documentation says a valid
low-reputation certificate can still show a proceed/abort warning. Reputation
determines whether Windows prompts or proceeds without that warning.

Sources: [Foundation certificate and approval policy](https://signpath.org/terms.html),
[signing-policy approvals](https://docs.signpath.io/projects),
[Windows and SmartScreen behavior](https://signpath.io/knowledge-base/windows-platform).

## Recommended sequence

1. Correct the README and release/download documentation for the existing
   Desktop product.
2. Add an accurate privacy policy and required Code signing policy with named
   roles; enable MFA for those people.
3. Inventory the exact NSIS payload and ask SignPath to confirm eligibility and
   the two-stage NSIS signing/approval design.
4. Apply to SignPath Foundation.
5. Only after acceptance, add the inert-by-default CI integration using the
   assigned organization, project, policy, artifact configuration, and API
   token.
6. Make signature verification and signed filenames release-blocking; never
   publish an unsigned fallback from a release intended to be signed.
