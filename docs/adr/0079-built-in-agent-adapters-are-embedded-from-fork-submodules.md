# Built-in Agent adapters are embedded from fork submodules

Status: accepted; supersedes the adapter installation and version pin of [ADR-0007](0007-codex-built-in-agent-lifecycle.md)

The Codex and Claude Code adapters are maintained forks (`OldKrab/codex-acp`, `OldKrab/claude-agent-acp`). Publishing each fork change to npm and then bumping OpenAIDE's pin made every adapter update three steps with a wait in the middle. OpenAIDE therefore takes the adapter code straight from the forks: each fork is a git submodule pinned to a commit, `npm run build:adapters` builds both forks into a gitignored directory, and the App Server embeds the built packages, so every App Shell that ships the App Server ships the adapters without packaging changes. The forks no longer publish to npm. Claude Code moved first; Codex followed once its fork restored saved permissions and the model on resume itself, which removed OpenAIDE's install-time session-recovery patch.

The native agents (the Claude runtime and native Codex, several hundred MB per platform) are still not bundled. On first use the App Server unpacks the embedded adapter into its managed runtime directory itself and checks every file against its SHA-256 digest recorded at build time, again on each launch; `npm ci` installs only the adapter's dependencies, including the native agent, from a lockfile that OpenAIDE generates from the fork's `package.json`. The native agent's version therefore follows the fork. Node.js, npm, and network access on first use remain prerequisites, and ADR-0007's installation lifecycle (locks, leases, atomic publish, Installing Agent Status, setup-required and failed states) is unchanged. A runtime directory is named `<adapter version>-<12-character fork commit>`, so each pointer move installs beside the previous one.

When a fork's CI passes on `main`, it dispatches an OpenAIDE workflow that moves the submodule, regenerates the lockfile, and opens a pull request that merges itself on green CI; a daily scheduled run is the fallback. The Android build and the Termux installer read the Claude SDK version from the committed lockfile instead of from npm.

## Considered Options

- **Keep npm publishing, automated.** It still leaves a publish and a pin bump between a fork merge and OpenAIDE.
- **Install the adapter through npm from an embedded `npm pack` tarball (`file:` dependency).** `npm pack` output is not byte-reproducible, and `npm ci` silently installs the cached package for the lockfile's integrity even when the tarball's content has changed, so a rebuilt adapter could run stale code.
- **Inline the adapter's JavaScript dependencies with esbuild.** It only shrinks the first install; the forks' own build output is what their CI tests.
- **Move the fork sources into this repository.** Upstream syncs and their scripts live in the fork repositories.
