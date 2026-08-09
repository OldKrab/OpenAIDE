# Changelog

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
