# OpenAIDE for VS Code

OpenAIDE runs local ACP-compatible coding agents from an inspectable task
workbench. Tasks keep chat, permissions, tool activity, Agent configuration, and
recovery state visible inside VS Code.

This extension is alpha software. Back up important work; task storage and Agent
integration behavior may change between alpha releases.

## Requirements

- VS Code 1.100 or newer
- A supported ACP Agent, such as Codex or OpenCode, authenticated separately
- Node.js and npm when an Agent must be launched through `npx`

Open the OpenAIDE activity-bar view, check Agent Settings, then create a Task.

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
