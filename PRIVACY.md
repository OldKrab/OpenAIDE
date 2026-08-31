# OpenAIDE privacy policy

Effective: 2026-08-31

OpenAIDE is a local-first agent workbench. The OpenAIDE project does not operate
an account service and does not collect analytics, crash reports, Task content,
workspace files, or diagnostics automatically.

## Data stored on your device

OpenAIDE stores Tasks, Chat and tool history, project and worktree records,
settings, Agent configuration, local diagnostics, and runtime coordination data
in per-user application storage. Project files remain where the user placed
them. Credentials are stored using the applicable App Shell's protected
credential facility when that integration supports it.

Uninstalling OpenAIDE does not automatically delete this data. OpenAIDE's
**Reset Task History** action removes the product history described in its
confirmation screen while preserving settings, credentials, Agent-owned
sessions, and project files. A user can remove all remaining OpenAIDE application
data manually after uninstalling.

## When data leaves your device

OpenAIDE makes network requests only for a feature the user selected or for the
limited release checks described below:

- When a user works with an Agent, OpenAIDE passes the prompt, selected Images or
  File Attachments, Agent tool traffic, and related session context to that
  Agent. The Agent may be a local program or may communicate with its own
  provider. The Agent and provider have their own privacy terms. Agent tools may
  access workspace files or networks according to the Agent's permissions and
  the user's approvals.
- On explicit use of a built-in Agent, OpenAIDE may use npm or npx to obtain its
  Agent package when the required local package is unavailable. The built-in
  Codex integration is version-locked and installed into per-user storage. npm's
  infrastructure receives the ordinary request and network metadata associated
  with these downloads.
- When production Desktop updates are enabled in an update-capable build, OpenAIDE may
  check the canonical update service after startup and when the user explicitly
  checks. The request contains only the update channel, current version,
  platform, and architecture, plus network metadata normally visible to a web
  service such as the IP address. It contains no installation identifier, Task
  content, workspace data, or product analytics. Windows release builds may use
  this feed before Authenticode signing is available because updater signatures
  and operating-system publisher signatures are independent. Current macOS and
  development builds do not contact the production update feed.
- Opening a release, documentation, or other external link sends a request to
  that destination through the user's browser.

OpenAIDE does not sell personal data and does not include advertising trackers.

## Diagnostics and support

Runtime diagnostics stay on the device. OpenAIDE exports troubleshooting data
only when a user explicitly creates a Support Export. The standard export is
redacted, but any raw history or trace that the user explicitly includes may
contain sensitive prompts, paths, workspace content, or Agent data. The user
chooses whether and where to share an exported file.

## Third-party services

Depending on the features selected, data may be processed by the selected Agent
and its provider, npm's package infrastructure, GitHub Releases, and the
OpenAIDE update feed hosted by GitHub Pages. OpenAIDE does not control those services' retention or
privacy practices.

## Changes and questions

Material changes to this policy are reviewed in the public repository. For a
non-sensitive privacy question, open an issue in the
[OpenAIDE repository](https://github.com/OldKrab/OpenAIDE/issues). Do not put
private Task data, credentials, or workspace content in a public issue; use the
private process in [SECURITY.md](SECURITY.md) for sensitive reports.
