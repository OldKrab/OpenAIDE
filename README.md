# OpenAIDE

OpenAIDE runs coding Agents that support the
[Agent Client Protocol (ACP)](https://agentclientprotocol.com/) and keeps each run
as a Task you can inspect, stop, and return to.

A Task keeps the Agent's responses, tool calls, terminal output, permission
requests, session configuration, and errors together. Work stays visible when a
run fails or is interrupted, and OpenAIDE does not silently retry actions that
may have changed your files.

OpenAIDE is published as a Desktop App for Windows x64 and macOS Apple Silicon,
and as a packaged VS Code extension. A local Web App is also available when
building from source. Every App Shell uses the same Rust App Server and shared
Frontend. A Mobile App Shell is planned but is not included today.

## Agents

Codex and OpenCode are built in. You can also add any ACP Agent that can be
launched over stdio as a Custom Agent in Settings. The official ACP site keeps a
[list of Agents that support ACP](https://agentclientprotocol.com/get-started/agents).

OpenAIDE checks an Agent's capabilities before using optional features and marks
unavailable operations as unsupported. For example, an Agent must support
session discovery before OpenAIDE can find and adopt its existing sessions.

## What it does

- Saves accepted Chat and Agent activity in local Task history.
- Shows streamed responses, folded tool details, terminal output, permission
  requests, and failures in their Chat order.
- Reopens saved Tasks and archives Tasks you no longer need in the main list.
- Finds and adopts existing Agent sessions when the Agent supports it.
- Stops running Tasks and can recover sessions left stuck after a crash.

When reporting a problem, run **OpenAIDE: Export Support Diagnostics** from the
VS Code Command Palette and attach the redacted Support Export when possible.

## Install the VS Code extension

In VS Code, open Extensions, search for **OpenAIDE**, and select **Install**.

For manual installation, download the VSIX for your platform from
[GitHub Releases](https://github.com/OldKrab/OpenAIDE/releases). Testing builds
are marked as prereleases on GitHub.

| Platform | Release file |
| --- | --- |
| Linux x64 | `openaide-vscode-linux-x64-VERSION.vsix` |
| Windows x64 | `openaide-vscode-win32-x64-VERSION.vsix` |
| macOS Apple Silicon | `openaide-vscode-darwin-arm64-VERSION.vsix` |

Other operating-system and CPU combinations are not currently packaged. VS Code
1.100 or newer is required.

### Manual VSIX installation

1. Download the VSIX matching your platform.
2. In VS Code, run **Extensions: Install from VSIX...** and select the file, or
   install it from a terminal:

   ```sh
   code --install-extension path/to/openaide-vscode-PLATFORM-VERSION.vsix
   ```

3. Open the OpenAIDE activity-bar view.
4. Check Agent Settings, then create your first Task.

Codex and OpenCode must be authenticated separately. On first explicit Codex
use, OpenAIDE installs its version-locked Codex integration in durable per-user
storage and reuses it on later launches. This initial install requires Node.js,
npm, and network access. The built-in Codex Agent never uses a `codex-acp`
executable from `PATH`; register a Custom Agent to use a different build.

Each VSIX bundles the matching App Server executable. Standalone App Server,
Web App archive, and container artifacts are not currently published.

## Install the Desktop App

Download the package for your platform from
[GitHub Releases](https://github.com/OldKrab/OpenAIDE/releases):

| Platform | Release file | Current trust status |
| --- | --- | --- |
| Windows x64 | `openaide-desktop-win32-x64-VERSION-unsigned.exe` | Unsigned until SignPath Foundation accepts the project |
| macOS Apple Silicon | `openaide-desktop-darwin-arm64-VERSION-unnotarized.dmg` | Ad-hoc signed, but not Apple-notarized |

Windows may show an unknown-publisher warning. macOS may require approval in
**System Settings → Privacy & Security** after the first launch. These packages
do not yet use the production in-app update feed; install a newer package from
GitHub Releases to update.

The Desktop installer adds the application and its bundled OpenAIDE App Server.
On Windows, selecting a WSL project also installs the bundled Linux App Server
under `~/.local/share/openaide` in that distribution. OpenAIDE stores Tasks,
settings, and local diagnostics in per-user application data. Uninstall the
Windows app through **Settings → Apps → Installed apps**, or move `OpenAIDE.app`
to Trash on macOS. Uninstalling does not remove saved user data or project files.

See the [privacy policy](PRIVACY.md) for local storage and network behavior and
the [code signing policy](CODE_SIGNING.md) for release trust and verification.

## Build from source

The local Web App launcher currently targets Linux and requires Bash and standard
Linux process utilities. To build the workspace, install:

- Node.js 24.18.0 with npm
- Rust 1.97.1
- VS Code 1.100 or newer when testing the extension

Install dependencies and verify the workspace:

```sh
npm ci
npm run check
npm run test
npm run build
```

Run the local Web App on loopback:

```sh
OPENAIDE_WEB_ALLOWED_HOSTS=localhost,127.0.0.1 npm run web:local
```

The default address is `http://127.0.0.1:5474`. Local deployment configuration
can override it. Inspect the active address and logs with:

```sh
bash deploy/local-web.sh status
bash deploy/local-web.sh logs
```

Launch a VS Code Extension Development Host:

```sh
npm run vscode:launch
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the development workflow and required
checks.

## Repository layout

- `openaide-rs/app-server`: Rust App Server for Task state, ACP orchestration,
  persistence, and shell-neutral workflows.
- `openaide-rs/app-server-protocol`: typed App Server Protocol records and
  TypeScript binding generation.
- `packages/app-server-client`: shared TypeScript App Server client and generated
  protocol bindings.
- `packages/app-shell-contracts`: shared App Shell and Frontend contracts.
- `packages/frontend`: shared Task, Chat, navigation, and settings interface.
- `apps/web`: local Web App shell and browser bootstrap server.
- `apps/vscode-extension`: VS Code App Shell.

## Releases

Pull requests are checked by GitHub Actions. A versioned tag reachable from
`main` builds and smoke-tests Linux x64, Windows x64, and macOS Apple Silicon
VSIX packages, then creates the canonical immutable GitHub Release according to
the [release policy](docs/release-policy.md). Stable releases are reconciled to
the VS Code Marketplace; Open VSX publication starts with the next stable
release. Prerelease tags such as `v0.0.2-beta.1` remain GitHub prereleases.

## Contributing and security

See [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request. Report
security vulnerabilities privately by following [SECURITY.md](SECURITY.md).
Free Windows code signing is provided by SignPath.io, certificate by SignPath
Foundation.

## License

OpenAIDE is licensed under AGPL-3.0-only. See [LICENSE](LICENSE).
