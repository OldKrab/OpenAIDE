# Open Remote WSL environment launch

Date: 2026-08-12

## Finding

Open Remote WSL and OpenAIDE Desktop share the same broad topology: a Windows client starts a server inside a selected WSL distribution, connects through loopback, and keeps Linux-side state and processes in that distribution. The environment behavior is split across two layers, however:

- Open Remote WSL invokes a generated Bash installer through its WSL process manager. The script installs and starts the remote extension-host server on `127.0.0.1`, writes its PID and token to Linux-side files, redirects server output to a Linux-side log, and emits a marker-framed result for the Windows client to parse.
- The VS Code remote server, rather than the Open Remote WSL launcher script, evaluates the user's default shell as an interactive login shell for the remote extension-host environment. This is why tools installed by NVM or configured in shell startup files are available to remote extensions.

OpenAIDE's bundled App Server does not contain VS Code's shell-environment import. Therefore starting it directly with `wsl.exe --exec /bin/sh` only inherits WSL's base environment. To provide the same user-visible behavior, Desktop must explicitly start the App Server through the WSL user's interactive login shell while keeping shell startup output separate from the App Server handoff stream.

## Production implications

- Resolve the WSL user's configured login shell, with `/bin/sh` as a defensive fallback.
- Start that shell with interactive and login flags so NVM-style setup in `.zshrc`/`.bashrc` is applied.
- Pass the installed App Server path as an argument, not interpolated shell text.
- Preserve the App Server's stdout as the authenticated handoff channel. Discard unrelated shell startup output because it can both corrupt framing and contain user data.
- Keep the existing Linux-side PID ownership and Windows-side timeout/cleanup path.

## Primary sources

- [Open Remote WSL `serverSetup.ts` at commit `1ad02c8`](https://github.com/jeanp413/open-remote-wsl/blob/1ad02c8bf6de65c84623e55f5fcbcd3416fc4e10/src/serverSetup.ts#L50-L103) — invokes the generated Bash setup script and parses marker-framed output.
- [Open Remote WSL server start and lifecycle](https://github.com/jeanp413/open-remote-wsl/blob/1ad02c8bf6de65c84623e55f5fcbcd3416fc4e10/src/serverSetup.ts#L270-L344) — loopback server, token/PID files, log redirection, readiness parsing, and lifecycle wait.
- [VS Code Remote Development troubleshooting](https://code.visualstudio.com/docs/remote/troubleshooting#_configuring-extension-or-runtime-to-start-with-your-shell-environment) — documents that the remote extension-host environment is evaluated from the default shell as an interactive login shell.
