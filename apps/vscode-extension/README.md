# OpenAIDE for VS Code

OpenAIDE runs as a workspace extension. In remote windows such as WSL, SSH, and
Dev Containers, its App Server and Agent processes run with the remote
workspace. The extension also contributes an explicit remote-placement default
so clients migrate installations created before OpenAIDE declared its workspace
extension kind.

OpenAIDE brings coding agents into a VS Code-native task workspace. Follow what
they do, approve sensitive actions, and return to saved task history without
leaving your editor.

## What you can do

- Run built-in Codex, OpenCode, and Claude Code agents, or configure a custom ACP agent.
- Follow chat, tool activity, and terminal output in one Task.
- Review permission requests before an Agent takes sensitive actions.
- Reopen saved Tasks and continue from their existing Agent sessions.
- Configure Agent models, modes, and other supported options inside VS Code.

## Getting started

1. Install and authenticate a supported coding Agent.
2. Open the OpenAIDE view from the Activity Bar.
3. Check Agent Settings, then create a Task and send your first message.

When a Task needs attention, OpenAIDE presents a notification inside VS Code's
workbench. The notification includes an **Open Task** action that returns to the
affected Task.

## Requirements

- VS Code 1.100 or newer
- A supported ACP Agent, such as Codex, OpenCode, or Claude Code, authenticated separately
- Node.js and npm when an Agent must be launched through `npx`

Claude Code requires Node.js 22 or newer and npm. Select **Claude Code** in Agent
Settings and use its sign-in action. OpenAIDE downloads the pinned ACP integration
on first launch; a separate Claude Code CLI installation is not required.

## Reporting a problem

Run **OpenAIDE: Export Support Diagnostics** from the Command Palette or use
**Export diagnostics** from a Task or Settings. The shared picker can save a
public-safe runtime/log bundle or add explicitly selected OpenAIDE session
history, associated ACP traces, and available Agent-native transcripts. It
remembers the last successful export folder, and then offers the repository's
GitHub Bug Report form as an optional action where you can attach the ZIP.

Sensitive sources are unchecked for generic exports and carry a warning because
they may contain prompts, responses, paths, tool output, and secrets. Referenced
workspace and attachment files are not copied. Review the saved bundle before
attaching it to a public issue.

For source builds, contribution instructions, security reporting, and license
details, see the [OpenAIDE repository](https://github.com/OldKrab/OpenAIDE).
