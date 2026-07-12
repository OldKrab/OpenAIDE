# OpenAIDE

OpenAIDE is an ACP-native Agentic Development Environment (ADE) for running local coding agents from an inspectable task workbench. It keeps chat, permissions, tool activity, task state, and agent setup visible and recoverable.

OpenAIDE currently provides a local Web App and a VS Code extension over the same Rust App Server and shared Frontend. Desktop and Mobile App Shells are planned, not included in the alpha.

## Alpha status

The current release line is `0.0.1-alpha`. It is for hands-on testing, not production use. Features may be incomplete, APIs and storage formats may change without migration support, and defects may cause lost local task history. Back up important work and report problems with a diagnostics export when possible.

OpenAIDE runs real ACP-compatible agents; it does not silently substitute a mock agent. Built-in integrations currently include Codex and OpenCode, with Custom stdio ACP agents configurable in Settings. Agent capabilities vary and unsupported operations should be shown in the UI.

## Install an alpha release

Download files from the repository's GitHub Releases page. Alpha versions are marked as prereleases.

### VS Code extension

The VS Code extension is the simplest packaged alpha to test. It requires VS Code 1.100 or newer.

1. Download the VSIX for your platform: `openaide-vscode-linux-x64-VERSION.vsix`
   or `openaide-vscode-win32-x64-VERSION.vsix`.
2. In VS Code, run **Extensions: Install from VSIX...** and select the file, or use:

   ```sh
   code --install-extension openaide-vscode-PLATFORM-VERSION.vsix
   ```

3. Open the OpenAIDE view and check Agent Settings before creating a task.

Codex and OpenCode must be authenticated separately. OpenAIDE first uses compatible agent commands already on `PATH` and may fall back to `npx`, which requires Node.js, npm, and network access on first launch.

Each VSIX bundles the matching App Server executable. Standalone App Server,
Web App archive, and container artifacts are not published in the current alpha.

## Build from source

Prerequisites:

- Node.js 24 with npm
- the stable Rust toolchain
- VS Code 1.100 or newer when testing the extension

Install dependencies and verify the workspace:

```sh
npm ci
npm run check
npm run test
npm run build
```

Run the local Web App:

```sh
npm run web:local
```

By default the repository's local deployment configuration determines its port. Inspect the active address and logs with:

```sh
bash deploy/local-web.sh status
bash deploy/local-web.sh logs
```

Launch a VS Code Extension Development Host:

```sh
npm run vscode:launch
```

## Repository layout

- `openaide-rs/app-server`: Rust App Server for task state, ACP orchestration, persistence, and shell-neutral workflows.
- `openaide-rs/app-server-protocol`: typed App Server Protocol records and TypeScript binding generation.
- `packages/app-server-client`: shared TypeScript App Server client and generated protocol bindings.
- `packages/app-shell-contracts`: shared App Shell and Frontend contracts.
- `packages/frontend`: shared task, chat, navigation, and settings interface.
- `apps/web`: local Web App shell and browser bootstrap server.
- `apps/vscode-extension`: VS Code App Shell.

## Releases

Pull requests are checked by GitHub Actions. A versioned tag builds Linux and
Windows VSIX packages and creates a GitHub Release according to the
[release policy](docs/release-policy.md). Stable tags also publish those packages
to the VS Code Marketplace; prerelease tags such as `v0.0.1-alpha.1` remain
GitHub prereleases.

## Contributing and security

See [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request. Report
security vulnerabilities privately by following [SECURITY.md](SECURITY.md).

## License

OpenAIDE is licensed under AGPL-3.0-only. See [LICENSE](LICENSE).
