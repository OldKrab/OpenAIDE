# Switching Computers Is In Place and Keeps Work Running

Status: accepted

The Desktop App and the Mobile App let the user switch the computer in use from the Task navigation, without visiting Settings and without restarting. Switching changes which App Server the App Shell renders; it never stops work on the computer left behind. This replaces two choices of [ADR-0063](0063-desktop-can-use-another-computers-app-server.md): that a change restarts the Desktop, and that a Desktop using another computer starts no App Server. It applies to every kind of computer: this one, a paired computer, and an SSH Computer ([ADR-0069](0069-ssh-computers-are-started-like-termux.md)).

## Choice

Exactly one computer is in use at a time, as before. Merging several App Servers' Projects and Tasks into one navigation remains rejected.

- **Switcher.** Once at least one computer is paired or added over SSH, a compact control sits at the end of the Settings row of the Task navigation. It names the computer in use with its connection state, and shows only an icon while this device is in use. It opens in place a list of this device first, then the other computers in the order they were added, with the one in use checked and the last item leading to Settings, Remote. Other computers' reachability is not probed. A user who never pairs sees no new control. The Desktop opens it with Ctrl/Cmd+Shift+S, and digits 1 to 9 pick a row. The shared Frontend owns it; each App Shell only applies the switch.
- **One row per App Server.** A row stands for one App Server, identified by its key, not for a route to it. When an SSH Computer turns out to host an App Server that is also paired, the two become one row with two routes; a switch tries the route that worked last and falls back to the other.
- **Connect first.** The switch connects to the new computer while the current one stays in use, shows Connecting with Cancel, and moves only once connected. An SSH prompt during that connection is answered there; cancelling it cancels the switch. A failure is shown in the list and leaves the user where they were.
- **Unsent input.** When the page holds an unsent draft, a selected attachment, or an upload in progress, the switch asks before discarding it. Otherwise it asks nothing.
- **Place.** Each App Shell remembers, per computer, the last Task, Project, or New Task Project the user had open, and returns there; a place that no longer exists, or a first visit, opens that computer's start page. This is presentation state kept on the device.
- **Work keeps running.** The Desktop keeps this computer's App Server held while another computer is in use, and releases it only when the Desktop quits. The Mobile App keeps the protection it gives the phone's own running Tasks while another computer is in use.
- **Attention.** Both App Shells notify for this device's Tasks, as well as the computer in use's. Activating such a notification switches back and opens the Task. Other computers are not watched.

## Mechanism

The Desktop keeps its process and replaces the connection: it ends the gateway and connection to the computer it leaves, starts or rejoins the target, and reloads the WebView so the Frontend, its caches, and shell capabilities are created again for one App Server, as the Mobile App already does. Each switch uses a new client instance identifier, so a server still holding the previous one in its reconnect grace does not hand back stale state. Connection failures are reported per computer rather than once per run.

The Mobile App sends the list of computers and the one in use to every page, and accepts reading that state and the two switch commands from any page; every other connection command stays limited to the Settings sections.

A switch emits structured start and terminal events under one correlation identifier from the choice through the reloaded Frontend's first ready state, with source and target kinds, route, outcome, duration, and classified error, and no names, keys, or addresses.

## Considered Options

- **Keep the restart.** Rejected: it made switching slow and lost the user's place, and the cost was paid on every switch.
- **Swap the connection inside the live page.** Rejected: the Frontend's session, caches, and shell capabilities are created once per load; reloading the WebView rebuilds them with the code that already exists.
- **Switch first, then report an unreachable computer.** Rejected: it leaves the user on a dead computer and could discard a draft for nothing.
- **Stop this computer's App Server when another is in use.** Rejected: switching would silently interrupt running Tasks and cut off this computer's own Remote Devices.
- **A switcher row or header shown to every user.** Rejected: most users never pair, and the control would make remote work look like the main path.
- **One row per route.** Rejected: two rows for the same Projects and Tasks invite the user to wonder which is real.

## Consequences

- A Desktop using another computer still runs its own App Server and the agents in it, so it uses this computer's resources until it quits.
- Unsent drafts are still lost on a confirmed switch. Keeping them per computer and Task is a separate decision.
- Two different App Servers on one machine, such as the Desktop's own state root and the default root an SSH Computer uses, appear as two rows told apart by their kind. Whether they should be one App Server belongs to ADR-0069 and the shared state root question of [ADR-0064](0064-a-terminal-command-keeps-the-app-server-reachable.md).
