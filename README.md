# OpenAIDE

OpenAIDE is an ACP-native Agentic Development Environment (ADE) for local development shells.

It lets developers run any ACP-compatible coding agent from an inspectable task workbench with Chat history, permissions, tool activity, App Server state, and settings for Agents, MCP Servers, and Skills. The goal is to keep agent work visible and recoverable across the Web App, Desktop App, and VS Code extension shells.

Examples include OpenCode, Codex CLI, Claude Agent, Gemini CLI, GitHub Copilot, and custom local agents that speak ACP.

## Status

OpenAIDE is early software. APIs, storage formats, and UI details may change before a stable release.

## Workspace

This repository contains:

- `openaide-rs/app-server`: Rust Backend seed for task state, ACP orchestration, persistence, and host-neutral behavior.
- `packages/app-server-client`: TypeScript App Server Protocol/client bindings seed.
- `packages/frontend`: Shared Frontend seed for task, navigation, and settings UI.
- `apps/vscode-extension`: VS Code App Shell seed.

## Development

Install dependencies:

```sh
npm install
```

Run checks:

```sh
npm run check
```

Run tests:

```sh
npm run test --workspaces --if-present
cargo test -p openaide-app-server
```

Build:

```sh
npm run build
```

Pull requests are validated by GitHub Actions. Version tags publish release
artifacts and a container image according to the [release policy](docs/release-policy.md).

Launch the extension development host:

```sh
npm run vscode:launch
```

## License

OpenAIDE is licensed under AGPL-3.0-only. See [LICENSE](LICENSE).
