# Terminals Are Shells on the App Server's Computer

Status: accepted

A user away from the computer, on a Remote Device ([ADR-0062](0062-remote-devices-are-app-server-clients.md)) or a Desktop App using another computer ([ADR-0063](0063-desktop-can-use-another-computers-app-server.md)), has no way to run a command there except by asking an Agent. A Terminal gives them an interactive shell on the App Server's computer.

## Choice

- A Terminal is the user's own login shell, run by the App Server in its own environment: inside the WSL distribution when the App Server runs there, PowerShell on native Windows. It belongs to no Task, and Agents neither see nor read it.
- It opens from a Project, starting in its root, or from a Task, starting in its Task Workspace. Navigation gains no entry. Several Terminals may be open, each closed explicitly.
- The App Server owns the shell. A Terminal survives its clients disconnecting and ends only when the user closes it, the shell exits, or the App Server stops. On reconnect a client receives the recent output, about 10,000 lines kept in memory and never written to disk.
- Every client that opens a Terminal sees it live and can type into it, like attaching to one `tmux` session.
- Open Terminals do not keep the App Server running. Stopping it, including for a Desktop Update, first warns that they will close.
- Every trusted client may open a Terminal. Opening, closing, and failures are logged as metadata; content never is.
- VS Code opens its own integrated terminal at the same folder instead.
- The App Server Protocol creates, lists, resizes, and closes Terminals. Keystrokes and output travel on a dedicated stream per Terminal, so typing never queues behind other traffic on a relayed path.

## Considered Options

- **Typing into Agent-run commands.** Rejected: ACP terminals have no input, and the protocol is moving toward Agent-owned, display-only terminals.
- **A Terminal that ends with its connection.** Rejected: a phone that locks its screen would kill every long command.
- **A Terminal that keeps the App Server running.** Rejected: it reintroduces the server that never stops, which [ADR-0064](0064-a-terminal-command-keeps-the-app-server-reachable.md) rejected.
- **A separate setting to allow Terminals.** Rejected: a trusted client can already run commands through a Custom Agent or MCP server, so the setting would look like protection without being one.
- **One writer at a time.** Left out: both clients are the same person.
- **Binding each stream's attach token to the client that asked for it.** Left out: the token reaches only a trusted client, over that client's own connection; it is single-use, expires in 30 seconds, and is never logged. Binding it would mean carrying client identity through every gateway in front of the stream, for no protection beyond what pairing already gives.

## Consequences

- A Remote Device's key is now directly a shell on the computer. Removing a device remains the only revocation.
- Terminals do not survive an App Server restart.
