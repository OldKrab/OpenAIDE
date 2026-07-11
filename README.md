# OpenAIDE

OpenAIDE is an ACP-native Agentic Development Environment (ADE) for running local coding agents from an inspectable task workbench. It keeps chat, permissions, tool activity, task state, and agent setup visible and recoverable.

OpenAIDE currently provides a local Web App and a VS Code extension over the same Rust App Server and shared Frontend. Desktop and Mobile App Shells are planned, not included in the alpha.

## Alpha status

The current release line is `0.0.1-alpha`. It is for hands-on testing, not production use. Features may be incomplete, APIs and storage formats may change without migration support, and defects may cause lost local task history. Back up important work and report problems with a diagnostics export when possible.

OpenAIDE runs real ACP-compatible agents; it does not silently substitute a mock agent. Built-in integrations currently include Codex and OpenCode, with Custom stdio ACP agents configurable in Settings. Agent capabilities vary and unsupported operations should be shown in the UI.

## Install an alpha release

Download files from the repository's [GitHub Releases](https://github.com/OldKrab/OpenAIDE/releases) page. Alpha versions are marked as prereleases.

### VS Code extension

The VS Code extension is the simplest packaged alpha to test. It requires VS Code 1.100 or newer.

1. Download `openaide-vscode-VERSION.vsix` from the release.
2. In VS Code, run **Extensions: Install from VSIX...** and select the file, or use:

   ```sh
   code --install-extension openaide-vscode-VERSION.vsix
   ```

3. Open the OpenAIDE view and check Agent Settings before creating a task.

Codex and OpenCode must be authenticated separately. OpenAIDE first uses compatible agent commands already on `PATH` and may fall back to `npx`, which requires Node.js, npm, and network access on first launch.

### Other release artifacts

- `openaide-app-server-linux-x64-VERSION` is the standalone Linux x64 App Server binary for shell and protocol integration testing. It is not a complete graphical application by itself.
- `openaide-web-assets-VERSION.tar.gz` contains static shared Frontend assets for packaging and integration. It is not a standalone Web App server.
- `SHA256SUMS` contains checksums for the downloadable files.
- `ghcr.io/oldkrab/openaide:VERSION` is the Linux container image containing the local Web App, App Server, and Codex ACP adapter. The container path is experimental and requires explicit local state, workspace, authentication, and host configuration.

Verify a downloaded artifact before installing it:

```sh
sha256sum --check SHA256SUMS
```

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

Pull requests are checked by GitHub Actions. A versioned tag builds release-mode artifacts, publishes a container image, and creates a GitHub Release according to the [release policy](docs/release-policy.md). Prerelease tags such as `v0.0.1-alpha.1` create GitHub prereleases.

## License

OpenAIDE is licensed under AGPL-3.0-only. See [LICENSE](LICENSE).
