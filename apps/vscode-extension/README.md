# OpenAIDE for VS Code

OpenAIDE brings coding agents into a VS Code-native task workspace. Follow what
they do, approve sensitive actions, and return to saved task history without
leaving your editor.

## What you can do

- Run ACP-compatible coding agents such as Codex and OpenCode.
- Follow chat, tool activity, and terminal output in one Task.
- Review permission requests before an Agent takes sensitive actions.
- Reopen saved Tasks and continue from their existing Agent sessions.
- Configure Agent models, modes, and other supported options inside VS Code.

## Getting started

1. Install and authenticate a supported coding Agent.
2. Open the OpenAIDE view from the Activity Bar.
3. Check Agent Settings, then create a Task and send your first message.

When a Task needs attention while the VS Code window is unfocused, OpenAIDE
uses its local System Notifications companion to present an operating-system
notification. The companion is installed automatically with OpenAIDE and runs
on the UI machine, including when the workspace is remote. Use **OpenAIDE:
Test System Notification** from the Command Palette to verify desktop delivery.

## Requirements

- VS Code 1.100 or newer
- A supported ACP Agent, such as Codex or OpenCode, authenticated separately
- Node.js and npm when an Agent must be launched through `npx`

## Reporting a problem

Run **OpenAIDE: Export Support Diagnostics** from the Command Palette. The
command saves a public-safe ZIP containing the current runtime snapshot,
minimal version/platform metadata, and up to 24 hours (2 MB per source) of
strictly allowlisted Extension and App Server log records. It then offers to
open the repository's GitHub Bug Report form, where you can attach the ZIP.

The bundle excludes prompts, Chat, file contents and paths, terminal output,
environment variables, secrets, raw errors, and raw protocol payloads. Review
the saved bundle before attaching it to a public issue.

For source builds, contribution instructions, security reporting, and license
details, see the [OpenAIDE repository](https://github.com/OldKrab/OpenAIDE).
