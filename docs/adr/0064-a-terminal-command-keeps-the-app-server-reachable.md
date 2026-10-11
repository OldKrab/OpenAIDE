# A Terminal Command Keeps the App Server Reachable

Status: accepted

A computer can be made reachable by its Remote Devices ([ADR-0062](0062-remote-devices-are-app-server-clients.md)) from a terminal, with no window on it. This answers the question ADR-0062 left open: when an App Server may outlive its App Shells.

## Choice

The App Server program has a `remote` command. While it runs, the App Server of that state root runs and its Remote Devices can connect. The command is available wherever the App Server is shipped; it is not a separate product or a Linux-only feature.

The App Server's lifetime rule does not change. It still stops after its last same-machine client leaves. The command is such a client, one without a window, so the App Server lives as long as the command runs or an App Shell is open, and no longer. Someone who opens an App Shell did not ask for a permanent server and does not get one.

The command runs in the foreground until it is stopped. Keeping it alive across a closed terminal or a reboot is the user's own tooling.

Pairing, listing, and removing devices are separate invocations run from another terminal. They join the running App Server and never start one, because a code or a list from an App Server that is about to stop is useless.

## Mechanism

- `remote` joins the running App Server, or starts one the way an App Shell does: a separate process, outside the terminal's interrupt, that publishes its endpoint record. It then initializes as a client of kind `terminal` and renews its liveness by heartbeat. The device list changes delivered with each heartbeat become its one-line reports.
- `remote pair` asks for an invite and prints the Pairing Code as text for pasting and, on a terminal, as a QR code. It prints only once Remote Access is on, so a shown code can be used.
- `remote devices` lists devices oldest pairing first; `remote remove <number>` removes by that number after a confirmation, or at once with `--yes`.
- Stopping detaches the client. With Tasks running, the first interrupt says so and the second stops.
- A device's self-reported name is stripped of control characters before it is printed.

The command acts on the state root it is given, the default one unless the environment names another. App Shells each use their own state root today, so reaching an App Shell's App Server from a terminal means naming that root.

## Considered Options

- **An App Server mode that never stops.** Rejected: it adds a second lifetime rule and a state that must be turned off again. A client that stays covers the same need with the rule that exists.
- **A short Pairing Code backed by a lookup service.** Rejected: the long part of the code is the computer's key, and resolving a short code to it needs a service run for every user, which pairing does not otherwise depend on. Pasting covers a computer and a QR code covers a phone.
- **Installing itself as a system service.** Left out: service managers differ per system and the foreground command composes with all of them.
- **Printing a code at every start.** Rejected as the default: codes would collect in logs and scrollback. `--pair` asks for it explicitly.

## Consequences

- A same-machine client kind exists that renders nothing and holds no secrets. Anything that picks a client to show a prompt must keep choosing by capability, not by presence.
- A terminal client counts as another client for a Desktop update's shutdown check, which is correct: stopping the App Server would cut off the devices it serves.
- Whether App Shells and the terminal should share one state root, and with it how different versions meet on one App Server, is not decided here.
- The QR code is drawn with the terminal's own glyphs, dark modules left empty, so it reads correctly on a dark background. A light terminal shows it inverted, which not every reader accepts; the text code is the fallback.
