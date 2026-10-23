# Computer switcher

Status: agreed design ([ADR-0070](../adr/0070-switching-computers-is-in-place-and-keeps-work-running.md)); implementation planned in four independently shippable slices.

## Pre-implementation findings

- Both App Shells already keep several computers. Switching lives only in Settings, Remote (`RemoteSection.tsx`), behind two different shell capabilities: `connectionSettings` on the Mobile App and `remoteComputers` on the Desktop.
- `SidebarComputer` renders in the Desktop title bar and in the sidebar actions of other shells, and only while another computer is in use. On the Mobile App it never appears on a fresh page: the native side sends connection state only while a Settings section is open (`WorkspaceConnectionBridge.java:34`, `SetupPagePolicy.java:18`).
- The Desktop applies a switch with a full process relaunch (`restart_desktop`, `main.rs`). Obstacles to switching in place: `desktop_bootstrap` returns a cached connection, the remote gateway and client cannot be stopped (`remote-client/src/gateway.rs`), the failure dialog fires once per run, and `clientInstanceId` is per process.
- Leaving this computer on the Desktop lets its App Server drain about 40 seconds later (heartbeat timeout plus reconnect grace), interrupting running Tasks and stopping its Remote Devices endpoint. The Mobile App leaves Termux running but drops the wake lock protecting local work.
- Drafts live only in Frontend memory, and attachment handles belong to a client instance; any switch loses both.
- No App Shell remembers the last place: the Desktop route is an in-memory closure, the Mobile App loads `/` after each switch.
- No command palette exists; macOS has a native menu, other platforms map Ctrl shortcuts in the WebView (`desktopCommands.ts`).
- `main.rs` is above the 800-line limit and `AppSurfaces.tsx` is close to it; both need an extraction before they grow.
- Switch diagnostics have no source/target, outcome, end-to-end duration, or correlation across the restart.

## Slice 1: shared switcher and Mobile App state on every page

- Replace `connectionSettings`' computer fields and `remoteComputers` with one `computers` shell capability: snapshot (this device, computers with id, name, kind, current, link), subscribe, `use(id)`, `cancel()`. Each shell adapts its native side to it. The Settings, Remote page reads the same capability.
- `ComputerSwitcher` in the sidebar footer's Settings row, shown once any computer exists: name and dot on another computer, icon on this device, words only for an unhealthy link. An upward listbox: this device, then computers in the order added, current checked, last item "Pair or manage computers…". Remove `SidebarComputer` and the Desktop title-bar lead.
- Ctrl/Cmd+Shift+S opens it (`desktopCommands` gains `switch-computer`, and macOS File ▸ Switch Computer…); arrows, Enter, and 1 to 9 inside.
- Mobile App: push computer state to every page; accept `state`, `use_computer`, and `local` from any route; all other commands stay Settings-only.
- Tests: switcher unit and DOM tests (unpaired shows nothing, local icon, remote name, keyboard, narrow truncation); Android bridge policy test for the widened commands; one wide and one narrow viewport check.

## Slice 2: Desktop switches in place and keeps its App Server

- Extract the connection ownership from `main.rs` into a module that owns the current target and can replace it: local handoff, or remote gateway. Give `Gateway` and `RemoteClient` a stop handle in `openaide-remote-client`.
- New async command applies a switch: connect the target, retire the previous connection, issue a new client instance id, return the new bootstrap; the WebView reloads into it. Failure reporting is per computer, not once per run.
- The App Server hold always targets this computer's App Server and ends only on quit or update; startup with another computer in use also starts it, so switching back, this device's notifications, and this computer's own Remote Devices never depend on an earlier visit. Sleep recovery keeps working for it while another computer is in use.
- Runtime environment changes keep their restart.
- Tests: remote-client gateway stop and reconnect against the real App Server edge; Rust tests for the switch state machine (success, failure keeps current, cancel); a Desktop shell test that the WebView reload receives the new connection.

## Slice 3: connect first, unsent input, and place

- Connect first on both App Shells: the target row shows Connecting with Cancel; failure is shown in the list; nothing changes until connected. Android `RemoteNode` connects the target before disconnecting the previous one.
- Before applying a connected switch, the Frontend asks when the page has a draft, a selected attachment, or an upload in progress; declining closes the new connection.
- Place: the Frontend records the last Task, Project, or New Task Project per computer id in device storage and restores it after the first replica load, falling back to the start page when it no longer exists. Settings is never restored. Add an inline TODO where drafts are dropped, naming per-computer draft persistence as the follow-up.
- Tests: switcher state for connect, fail, cancel, and confirm; place restore with an existing and a missing Task.

## Slice 4: notifications for this device

- Depends on `taskAttention/wait` from `shushakov/paired-task-notifications`; rebase after it merges.
- Mobile App: the background service watches the phone's App Server as well as the selected computer, and keeps the wake lock while local Tasks run. A tap on a local notification switches to this phone and opens the Task.
- Desktop: while another computer is in use, the Desktop shell keeps a held `taskAttention/wait` on this computer's App Server and notifies through the existing Desktop notification manager; activation switches back and opens the Task.
- Tests: the watch's baseline, in-sight, and one-per-Task rules for the second source; activation routing.

## Diagnostics

Each switch emits `computer_switch` start and terminal events under one correlation id carried across the reload: source and target kind, route, attempt, outcome (`switched`, `failed`, `cancelled`, `declined`), duration to the reloaded Frontend's first ready state, and classified error. No names, keys, addresses, or paths. The Desktop log and Android diagnostics both record it, and Support Export includes the Desktop's.

## Open questions

- SSH Computers join the list once ADR-0069 lands; the capability's `kind` and route fields are shaped for them now.
