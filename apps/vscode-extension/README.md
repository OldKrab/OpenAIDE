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

For source builds, contribution instructions, security reporting, and license
details, see the [OpenAIDE repository](https://github.com/OldKrab/OpenAIDE).
