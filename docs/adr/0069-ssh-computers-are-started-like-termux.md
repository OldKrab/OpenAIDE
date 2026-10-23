# SSH Computers Are Started Like Termux

Status: proposed

The Desktop App and the Mobile App can make a Linux computer reachable over SSH the place where agents work. The App Shell installs the App Server there, starts it, and reaches it the way the Mobile App reaches its App Server in Termux: it holds the per-process token and is a client of that App Server, with the loopback endpoint carried through the SSH connection instead of a local socket. No Pairing Code, key exchange with the App Server, or relay is involved; the user's SSH access is the proof. Pairing with a computer ([ADR-0063](0063-desktop-can-use-another-computers-app-server.md)) stays the way to reach an App Server someone else started.

## Choice

An SSH Computer is one more choice next to this computer and paired computers. Exactly one is in use at a time, and changing it restarts the App Shell as ADR-0063 does. The first release supports Linux on x64 only.

The App Server on an SSH Computer uses that computer's default state root, the one the `remote` command uses ([ADR-0064](0064-a-terminal-command-keeps-the-app-server-reachable.md)). A computer has one OpenAIDE: Projects and Tasks started over SSH are the ones its paired devices see.

### Versions

The App Shell installs its own App Server release next to earlier ones, under one directory per version in the user's home. On connecting it finds one of four cases:

- **Same version running, or none**: it attaches, or starts one.
- **Older version, compatible protocol**: it attaches like a Remote Device, by protocol compatibility ([ADR-0062](0062-remote-devices-are-app-server-clients.md)), and offers an update.
- **Older version, incompatible protocol**: it cannot attach until the computer is updated.
- **Newer version**: it refuses and asks the user to update this App Shell.

An update happens only after the user confirms. The confirmation lists the other clients attached to that App Server with their kind, name, and version, marks the ones the update will strand, and counts running and waiting Tasks. Confirming sends the existing replacement request. An App Shell never replaces a newer App Server with an older one, so two clients of different versions cannot replace each other in turn.

### Lifetime

The App Server's lifetime rule does not change: it stops after its last same-machine client leaves. The App Shell starts a windowless keeper on the SSH Computer, a client like the `remote` command, outside the SSH session. The keeper stays while any of these holds:

- a Task is running;
- a Task has waited for the user for less than 24 hours;
- an App Shell was attached within the last 30 minutes.

When none holds, the keeper leaves and the App Server stops by the existing rule. A Task that was waiting is then interrupted and is resumed later the way any Task with a Native Session is. The App Shell also offers an explicit stop, which warns about running Tasks.

## Mechanism

- One Rust implementation in `openaide-remote-client` does the launch: detect the system and architecture, upload and verify the release, start the App Server through the user's login shell, read the endpoint line, apply the version cases, start the keeper, and forward the endpoint. It needs only two things from SSH: running a command and opening a channel to a loopback port.
- The Desktop App provides those through the system `ssh`, so the user's SSH configuration, jump hosts, agent, hardware keys, and interactive prompts keep working. Prompts are shown in the App Shell and never stored.
- The Mobile App embeds an SSH client. The phone generates its own key, kept encrypted with Android Keystore like its pairing key, and shows the public key to add on the computer. A password may be used once to add that key, and is not stored. Jump hosts are not supported on the phone at first.
- The App Shell downloads the matching release asset once, checks it against the SHA-256 built into the App Shell, and uploads it over SSH. The SSH Computer needs no internet access to receive it. The Windows Desktop reuses the Linux App Server it already bundles for WSL.
- Agents are installed and signed in on the SSH Computer, as in Termux. When Node is missing, a pinned Node runtime is installed in the user's home without elevated rights.
- The launcher detects a login manager that ends a user's processes when the SSH session closes and names the setting that keeps the keeper alive.
- The App Server Protocol gains the list of attached clients with kind, name, product version, and protocol version, as a minor version change.
- On the Mobile App, Task notifications for an SSH Computer come from the same foreground-service watch used for a paired computer, held over the SSH connection.
- Each launch stage, from connecting to forwarding, emits structured start and end events with a correlation identifier, outcome, duration, attempt count, and classified error, and no host names, user names, paths, or command output.

## Considered Options

- **SSH only to install, then pairing over iroh.** Rejected as the main path: traffic would depend on public relays, which many networks block, while SSH already reaches the computer. Pairing from an attached App Shell remains available.
- **A separate state root for SSH.** Rejected: it would split one computer's Projects and Native Sessions in two.
- **An idle timer in the App Server.** Rejected for the reason ADR-0064 gives: a second lifetime rule. The keeper is a client and decides only when to leave.
- **Updating without confirmation.** Rejected: replacement ends running Tasks and can strand other clients.
- **One embedded SSH client everywhere.** Rejected for the Desktop, because it would break existing SSH configurations.
- **The SSH Computer downloading the release itself.** Rejected: it needs internet access on the computer and a second download path.

## Consequences

- Secrets a user enters for an Agent or MCP server stay in the App Shell that entered them (ADR-0063), so Tasks that run while no App Shell is attached cannot read them. The App Shell says so. Moving that storage to the App Server's computer is a separate decision.
- Linux on arm64, macOS, and Windows computers need their own App Server builds and launch paths.
- The macOS Desktop and the Mobile App need network access once per release to fetch the Linux App Server.
- A waiting Task can keep the App Server alive for up to a day after its user left.
