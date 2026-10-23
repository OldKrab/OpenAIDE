# Terminals

Status: contract agreed; presentation approved by prototype; steps 1–7 implemented, not yet released.

## Agreed scope

The decision and rejected options are in [ADR-0071](../adr/0071-terminals-are-shells-on-the-app-servers-computer.md).

- A Terminal is the user's login shell on the App Server's computer, run in the App Server's environment (WSL distribution when App Server runs there, PowerShell on native Windows). It belongs to no Task and is invisible to Agents.
- Entry points: **Open terminal** in a Project's menu (starts in its root) and a **Terminal** action on the Task Page header (starts in its Task Workspace). Navigation gains no entry.
- Several Terminals may be open. App Server owns each shell; it survives client disconnects and ends on close, shell exit, or App Server stop.
- Reconnecting replays about the last 10,000 lines, held in memory only.
- Every client viewing a Terminal sees it live and may type.
- Closing a Terminal whose shell has a running foreground command asks for confirmation.
- Open Terminals do not keep App Server alive. Stopping App Server, including for a Desktop Update, first warns how many Terminals will close.
- Any trusted client may open a Terminal. Logs record open, close, exit, and failure as metadata only.
- VS Code opens an integrated terminal at the same folder instead of rendering one.

## Approved presentation

Approved from the `terminal` UI prototype (variant `page` on Desktop, chip tabs on Android).

- **Desktop and Web:** the Terminal is a full Task Page destination, like Project files. A header holds **← Conversation**, the title **Terminal** with `Project · computer` beneath it, and the active Terminal's current folder at the right. Below it, a tab strip lists Terminals: a state dot (running, idle, exited), a monospace title (the running command, otherwise the folder name), and a close button; a **+** opens another Terminal. The terminal fills the rest.
- **Task header:** a **Terminal** button with the open-Terminal count sits beside Project files.
- **Android:** the same destination, full screen. The top bar holds back, **Terminal** with the computer name, and a keyboard show/hide button. Under it, a horizontally scrolling row of chip tabs with the same state dots and a **+**. A key row sits directly above the soft keyboard: ctrl (sticky, highlighted while armed), esc, tab, ^C, arrows, `|`, `~`, `/`, `-`. Tapping the key row never dismisses the keyboard.
- **Closing a running Terminal:** a small confirmation anchored to its tab: "Stop `<command>`?", "Closing this terminal ends the command.", **Cancel** / **Close terminal**.
- **Reconnecting:** a warning-tinted strip above the terminal, "Reconnecting to `<computer>`… Your shells keep running."; input is ignored until reconnected. On reconnect the retained output reappears.
- **No Terminals:** an empty state with **New terminal in `<Project>`**.

## Pre-implementation findings

- App Server already runs one interactive PTY: Agent sign-in (`agent/auth_terminal.rs`, `portable-pty` with `TERM=xterm-256color`). Its byte path is a reverse request polled about every 100 ms with 32 KiB frames and a 30 s liveness deadline. That suits sign-in but not an interactive shell, so Terminals need a dedicated stream.
- The local HTTP listener already serves side channels beside the reliable session (`listener/uploads.rs`, `listener/viewer_downloads.rs`) and a WebSocket session link (`listener/session_socket.rs`). Remote Devices reach the same listener through the iroh bridge, which carries WebSocket upgrades, so a per-Terminal WebSocket route serves both same-machine and Remote clients without a second transport.
- The Frontend already bundles `@xterm/xterm` and `addon-fit` (used by `settings/AgentAuthTerminal.tsx`).
- `ShellCapability::OpenTerminal` is defined and advertised by clients but read nowhere; it is the natural capability for the VS Code path.
- Update Shutdown Readiness is decided in `protocol_edge/client_handlers.rs` and `responses.rs`; the Terminal warning joins that check.

## Implementation outline

1. **App Server Terminal registry** (done). `app-server/src/terminals.rs` owns the open Terminals; `terminals/shell.rs` runs one login shell (`$SHELL -l`, else `/bin/sh -l`; `powershell.exe -NoLogo` on Windows) with reader, writer, and monitor threads; `terminals/replay.rs` retains the newest 10,000 lines within 2 MiB, cut at a newline. The PTY child and its process-group teardown moved from `auth_terminal.rs` to the shared `pty_child.rs`. Busy detection compares the PTY foreground process group with the shell; the command name and current folder come from `/proc` on Linux. On Windows a child process of the shell marks it busy and names the command (PowerShell cmdlets run in-process and count as idle); the folder stays the start folder there, and macOS reads neither yet (TODOs in `shell.rs`). App Server stop closes every Terminal.
2. **Protocol** (done, protocol 2.3). `terminals/open` (Project or Task target, cols, rows → summary and attach), `terminals/attach` (→ a new attach), `terminals/close` (`force` after confirmation; `conflict` while a command runs). The list is the `terminals` subscription scope with `terminalCollectionUpdated` events; each summary carries id, Project, folder, command, status (`idle`, `busy`, `exited`), exit code, and open time. Resize travels on the socket. The folder is always resolved on App Server from the Project root or the Task Workspace.
3. **Byte stream** (done). A WebSocket upgrade whose path ends in `/terminal` reaches `listener/terminal_socket.rs`. The client sends a text `hello { token, cols, rows }` with a single-use attach token valid 30 s; the server answers `ready`, sends the replay as binary frames, then live output. Client binary frames are input; text `resize { cols, rows }` resizes, the latest client winning. Close codes: 1000 the Terminal ended, 4400 protocol violation, 4401 bad token (ask `terminals/attach`), 4408 the viewer fell behind and must reattach. A viewer more than 256 output chunks behind is dropped rather than stalling the shell or other viewers. The server sends a text `ping` every 15 s, busy or quiet, and the client answers `pong`; each side treats 45 s without a message as a lost connection. The pings are text because a browser answers WebSocket control pings itself without telling the page.
4. **Lifetime and shutdown.** Terminals end with App Server (done: `SharedRpcGateway::shutdown` closes them). Desktop Update readiness reports the open-Terminal count, and Settings appends "N terminals will close." to the active-work note (done). Desktop Quit (the macOS menu and the Windows window close) asks "N terminals will close." in a native dialog before its 5 s exit fallback starts (`desktop_quit.rs`), from a count the Frontend pushes as the list changes. It does not ask when Desktop uses another computer's App Server; it still asks when a connected Remote Device would keep this App Server running (TODO in `desktop_quit.rs`).
5. **Frontend** (done). `components/terminals/` holds the page, its parts, the xterm screen, and the Task header button; `intents/terminalIntents.ts` holds open, attach, and close; `services/terminalSockets.ts` carries one viewer's bytes and reattaches with backoff after a lost socket or a socket that never answers `ready` within 10 s. The list is App State from the `terminals` scope. Each App Shell supplies the socket URL through `FrontendShell.terminals`; the Web dev server, the Android gateway, and the Remote Client gateway forward the `/terminal` upgrade, which the attach token alone authorizes. The page opens a Terminal on entry when its Project has none.
6. **VS Code** (done). `FrontendShell.nativeTerminal` posts `shell.terminal.open` with the Task Workspace or Project root; the extension host requires an absolute folder and calls `vscode.window.createTerminal`. It uses an App Shell capability rather than `ShellCapability::OpenTerminal`, because the Frontend, not App Server, chooses the presentation. Clients no longer advertise `openTerminal`; the variant still deserializes for 2.2 clients and goes with the next major protocol version.
7. **Observability** (done). App Server logs `terminal_open_started`/`_completed`, `terminal_attach_issued`, `terminal_closed`, status changes and exits, `terminal_socket_opened`/`_closed`/`_rejected` with byte counts and reason codes, and `terminal_resize_failed`. The Frontend logs `terminal_intent_*` and `terminal_stream_*` with Terminal id, attempt, close code, and duration. No content, commands, paths, tokens, or environment values.

## Verification targets

- Create from a Project and from a Task Workspace on Linux, macOS, native Windows, and WSL; the login shell and folder are correct.
- Disconnect and reconnect a Remote Device mid-command: the shell keeps running, replay is bounded, live output resumes.
- Two clients on one Terminal: both see output, both can type, resize follows the most recent active client.
- Close an idle Terminal, a Terminal with a running command (confirmation), and a Terminal whose shell exited.
- App Server stop and Desktop Update readiness with open Terminals show the warning; Terminals do not keep App Server alive.
- A slow viewer on a relayed path does not stall input for another viewer.
- Logs contain the lifecycle events and no Terminal content.
- Shared Frontend at a wide desktop viewport and a narrow phone viewport, including the key row above the real Android keyboard.
