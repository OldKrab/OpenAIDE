# OpenAIDE

OpenAIDE is an
[Agent Client Protocol (ACP)](https://agentclientprotocol.com/)-native Agentic
Development Environment for running Codex, OpenCode, and Custom ACP-compatible
Agents locally as durable, inspectable Tasks.

OpenAIDE treats Agent work as a recoverable Task, not a disposable chat tab. Each
Task keeps its Agent-owned session, streamed Chat, permission decisions, tool and
terminal activity, configuration, and recovery state visible and attributable.
A local Rust App Server owns Task lifecycle and persistence instead of leaving
that state inside a transient UI.

The packaged alpha runs inside VS Code. A local Web App is available when
building from source. Both use the same Rust App Server and shared Frontend;
Desktop and Mobile App Shells are planned, not included today.

ACP is the open protocol that lets editor-like clients communicate with coding
Agents. OpenAIDE uses real ACP sessions and capabilities rather than substituting
a mock Agent or reducing every integration to the same hard-coded feature set.

## Why OpenAIDE

- **ACP-native Agents:** use the built-in Codex and OpenCode integrations or add
  a Custom stdio ACP Agent in Settings. OpenAIDE discovers Agent capabilities and
  exposes supported session configuration instead of assuming one provider.
- **Durable Tasks:** keep accepted Chat and Agent activity in local Task history
  that can be reopened, inspected, and archived.
- **Agent-owned session continuity:** discover and adopt existing Agent sessions
  when the Agent supports it, while keeping their activity inspectable as Tasks.
- **Inspectable execution:** follow streamed responses, folded tool details,
  terminal output, permission requests, and failures in their Chat order.
- **Explicit recovery:** keep interrupted work visible, stop running Tasks, and
  recover locally stalled sessions without silently resending Agent work.
- **Local-first state ownership:** keep Task workflow and history in the local App
  Server rather than making a transient Frontend the only owner of Agent work.

Agent capabilities vary. OpenAIDE shows unsupported operations in the interface
when the connected Agent does not provide them.

## Alpha status

> [!WARNING]
> The `0.0.1-alpha` release line is for hands-on testing, not production use.
> Features may be incomplete, APIs and storage formats may change without
> migration support, and defects may cause lost local Task history. Back up
> important work.

When reporting a problem, run **OpenAIDE: Export Support Diagnostics** from the
VS Code Command Palette and attach the redacted Support Export when possible.

## Install the VS Code alpha

Download the VSIX for your platform from
[GitHub Releases](https://github.com/OldKrab/OpenAIDE/releases). Alpha versions
are marked as prereleases.

| Platform | Release file |
| --- | --- |
| Linux x64 | `openaide-vscode-linux-x64-VERSION.vsix` |
| Windows x64 | `openaide-vscode-win32-x64-VERSION.vsix` |
| macOS Apple Silicon | `openaide-vscode-darwin-arm64-VERSION.vsix` |

Other operating-system and CPU combinations are not packaged in the current
alpha. VS Code 1.100 or newer is required.

1. Download the VSIX matching your platform.
2. In VS Code, run **Extensions: Install from VSIX...** and select the file, or
   install it from a terminal:

   ```sh
   code --install-extension path/to/openaide-vscode-PLATFORM-VERSION.vsix
   ```

3. Open the OpenAIDE activity-bar view.
4. Check Agent Settings, then create your first Task.

Codex and OpenCode must be authenticated separately. OpenAIDE first uses
compatible Agent commands already on `PATH` and may fall back to `npx`, which
requires Node.js, npm, and network access on first launch.

Each VSIX bundles the matching App Server executable. Standalone App Server,
Web App archive, and container artifacts are not published in the current alpha.

## Build from source

The local Web App launcher currently targets Linux and requires Bash and standard
Linux process utilities. To build the workspace, install:

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

Pull requests are checked by GitHub Actions. A versioned tag builds Linux x64,
Windows x64, and macOS Apple Silicon VSIX packages and creates a GitHub Release
according to the [release policy](docs/release-policy.md). Stable tags also
publish those packages to the VS Code Marketplace; prerelease tags such as
`v0.0.1-alpha.1` remain GitHub prereleases.

## Contributing and security

See [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request. Report
security vulnerabilities privately by following [SECURITY.md](SECURITY.md).

## License

OpenAIDE is licensed under AGPL-3.0-only. See [LICENSE](LICENSE).
