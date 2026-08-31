# Changelog

## 0.4.4 - 2026-08-31

### Desktop updates

- Fixed Restart and update so installer handoff continues when the App Server closes before its final detach response reaches the Desktop UI.
- Simplified Windows and macOS desktop download filenames.

## 0.4.3 - 2026-08-31

### Desktop updates

- Enabled the first production Windows in-app update path for installations running OpenAIDE Desktop 0.4.2.

## 0.4.2 - 2026-08-31

### Desktop updates

- Fixed Windows release packaging so updater-enabled installers include the configured update identity and can publish signed update artifacts.

## 0.4.1 - 2026-08-31

### Desktop updates

- OpenAIDE Desktop on Windows can now detect signed application updates, show release notes, download the installer, and request a restart to finish updating.
- Improved packaged Windows Codex startup, path handling, task recovery, diagnostic downloads, title-bar interactions, and live Codex installation status.
- Prepared Windows and macOS desktop artifacts for the free code-signing path while preserving explicit trust labels for unsigned or unnotarized downloads.

### Agents and tasks

- Added native ACP subagent histories and follow-up support.
- Fixed custom Agent label reconciliation, task-navigation refresh loops, collapsed-sidebar restoration, and first-click auto-approval behavior.
- Improved Composer feedback when configuration changes and adjusted task-navigation limits for larger projects.

## 0.4.0 - 2026-08-27

### Features

- Preview and archive older Project Tasks in bulk while protecting pinned, active, queued, pending-request, and open-elsewhere Tasks.
- Rename and pin unadopted Native Sessions, preserving that metadata when they become Tasks.

### Bug Fixes

- Preserve the selected Project across VS Code New Task panels without letting stale panels overwrite the App Server default.
- Keep the built-in Codex launcher available when legacy Custom Agent data uses its reserved identifier.
- Recover Prepared Task sessions before changing Agent options, with immediate loading, failure, attachment, and retry feedback in Composer.
- Preserve first-send attachments when workspace paths resolve to the same canonical Project.
- Keep the built-in Codex launcher available when legacy Custom Agent data uses its reserved identifier.
- Preserve first-send attachments when workspace paths resolve to the same canonical Project.
- Keep active Native Sessions attached when ACP session discovery times out.
- Keep Task permission controls available while unrelated global state recovers.
- Save Support Export bundles correctly from VS Code.
- Prevent wrapped Agent Plan entries from overlapping.

### Chores

- Allow manual artifact builds to target one VSIX platform while release builds continue to cover every platform.
- Strengthen Linux VSIX packaging gates and the bundled Codex startup smoke test.

**Full Changelog**: https://github.com/OldKrab/OpenAIDE/compare/v0.3.0...v0.4.0

## 0.3.0 - 2026-08-19

### Features

- Open Agent file references in an in-Task viewer on Web and Desktop, with source highlighting and one-click line quotes into Chat.
- Browse folders directly when adding a Web project.
- Choose a per-Task permission policy that can automatically approve safe one-time Agent requests.
- Export selected recent session diagnostics from Settings, with sensitive Agent artifacts kept opt-in.

### Bug Fixes

- Keep the routed Task's Chat visible after an App Server reconnect.
- Preserve Agent response boundaries around User messages and permission requests.
- Keep steered Tasks active while the replacement Agent response is still running.
- Deliver queued Web uploads after their source files move into App Server-managed storage.
- Keep the File Viewer and line quoting usable on narrow Task pages.
- Reflect live Agent connection status in Settings.

### Chores

- Capture production-safe ACP process exit metadata for support diagnostics.
- Generate release notes at release time and reuse them for the changelog and GitHub Release.

**Full Changelog**: https://github.com/OldKrab/OpenAIDE/compare/v0.2.5...v0.3.0

## 0.2.5 - 2026-08-18

### Bug Fixes

- Batch streamed Agent text durability so long responses remain responsive.
- Preserve ordered live Chat updates while coalescing streamed text writes.
- Keep prepared Tasks aligned with the current Agent configuration at startup.
- Render Cursor ACP tool activity with the correct structured details.

## 0.2.4 - 2026-08-17

### Bug Fixes

- Recover shared ACP agents when their process exits during session startup.
- Persist the ACP developer trace setting across App Server restarts.
- Add metadata-only diagnostics for ACP connection, initialization, requests, and options.

## 0.2.3 - 2026-08-17

### Bug Fixes

- Restore the VS Code Developer settings unlock after a webview reload.
- Remember the latest New Task Agent selection as the next default.

## 0.2.2 - 2026-08-17

### Bug Fixes

- Recover agent startup after an ACP session times out by replacing the unresponsive shared process before retrying.

### Chores

- Harden release-note validation and canonical release checks.

## 0.2.1 - 2026-08-17

### Features

- Add the native Desktop App for Windows and macOS, with self-contained preview installers and Windows WSL runtime support.
- Use an OpenAIDE-owned Codex ACP adapter with a pinned fork release for reproducible agent launches.
- Share task, settings, and shell behavior across the VS Code, web, and desktop surfaces.

### Bug Fixes

- Harden ACP session recovery, Agent configuration lifecycle, and Native Session persistence across reloads and attachment recovery.
- Persist project state and prepared-Task transitions safely across repeated writes.
- Fix worktree path copy placement and desktop packaging and startup behavior.

### Chores

- Validate exact-version VSIX and desktop artifacts, including bundled App Server startup, WSL packaging, and platform smoke tests.
- Align supported Node, Rust, VS Code, and packaging toolchains.

## 0.1.3 - 2026-08-14

### Fixes

- Serialize New Task prepared-task replacement and release stale late acquires before switching context.

## 0.1.2 - 2026-08-14

### Fixes

- Prevent custom-agent replacement saves from hanging while stale prepared tasks are cleaned up.
- Expose Cursor ACP model, thinking effort, and fast-mode controls in the task composer.

## 0.1.1 - 2026-08-14

### Bug Fixes

- Prevent a failed custom Agent command from freezing the Settings page.
- Keep broken Agents editable and deletable while their status is checked.
- Refresh only the changed Agent and clean stale prepared Tasks after Agent changes.

## 0.1.0 - 2026-08-12

### Features

- Render Mermaid diagrams directly in Agent Chat with reliable diagram inspection.
- Add copy actions to Markdown quotes while preserving their Markdown formatting.
- Improve ACP permission requests with distinct decisions, grouped repeats, and complete Tool details including edit diffs.
- Make task search feedback clearer when filtering and navigating results.

### Bug Fixes

- Ensure legacy OpenAIDE installations move to the remote workspace in WSL, SSH, and Dev Containers.
- Make Native Session recovery explicit, restore controls sooner, and provide clear reload and retry paths.
- Keep Agent links clickable while responses stream.
- Make trackpad image zoom easier to control and improve quoted-text contrast in the dark theme.
- Hide the queue action when the queue is empty and fix Plan panel scrolling.

## 0.0.2 - 2026-08-09

### Features

- Add durable message queuing so follow-up messages can wait safely for the active turn to finish.
- Add project management and project-scoped task creation.
- Add selected-text quotes and ACP session forks for carrying context into new work.
- Let users close incomplete plans and improve task activity summaries.

### Bug Fixes

- Improve App Server recovery, activation, prompt settlement, and session refresh behavior.
- Keep Native Session options coherent across reloads and attachment recovery.
- Fix clipboard handling in VS Code and improve mobile task, chat, and New Task layouts.
- Improve chat scrolling, spacing, code wrapping, image zoom, quote popups, and sidebar previews.

### Chores

- Expand production-safe lifecycle diagnostics and bound persisted diagnostic data.
- Harden the release pipeline and align supported Node, VS Code, and packaging toolchains.

## 0.0.1 - 2026-07-31

### Highlights

- Ship the first stable OpenAIDE release for VS Code, with native packages for Linux x64, Windows x64, and macOS Apple Silicon.
- Run Codex, OpenCode, and custom ACP Agents in inspectable Tasks that keep Agent messages, plans, tool activity, terminal output, permission requests, questions, and failures together.
- Reopen durable Tasks, archive work you no longer need in the main list, adopt supported Agent sessions, and recover sessions left stuck after a crash.

### Experience

- Navigate Projects and Tasks with durable pinning, Composer History, responsive Agent settings, context-usage details, Agent plans, and subagent activity.
- Preview Tool images, copy Markdown code blocks, inspect semantic Tool summaries, and receive native OS notifications when background Tasks need attention.
- Preserve pending questions and permissions through configuration changes, restore composer focus reliably, and keep compact Chat activity readable across constrained layouts.

### Reliability

- Persist accepted Chat and Task activity before rendering it, expose failures instead of silently retrying uncertain actions, and provide privacy-safe support diagnostics.
- Recover empty prepared Tasks safely, synchronize newer native-session history, and improve fallback file uploads with faster chunking, transient-timeout retries, and actionable stalled-upload errors.
- Build every release from one canonical version, stamp each packaged manifest exactly, and identify unofficial VSIX builds by their source commit.

[Full 0.0.1 changes](https://github.com/OldKrab/OpenAIDE/compare/v0.0.1-beta.4...v0.0.1)
