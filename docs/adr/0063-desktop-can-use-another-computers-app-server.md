# Desktop Can Use Another Computer's App Server

Status: accepted

The Desktop App on one computer can work with the App Server of another computer, the same way the Mobile App does ([ADR-0062](0062-remote-devices-are-app-server-clients.md)). It pairs by Pairing Code, is trusted by its own key pair, and reaches the other App Server over iroh. No account, address, or password is involved.

## Choice

A Desktop keeps its own App Server. The user chooses one place where agents work: this computer, or one of the computers it is paired with. Exactly one is in use at a time, and the choice is a Desktop bootstrap preference next to the runtime environment, so changing it restarts the Desktop like an environment change does. Pairing with a computer selects it.

A Desktop pairs only in the invite direction: the other computer shows a code and this Desktop spends it. Between two computers either direction costs the user the same copy and paste, so the join-request direction and its listening endpoint are left out.

When a paired computer is in use the Desktop starts no App Server. Startup first reaches the paired computer, so an unreachable computer is named on the startup screen together with the way back to this computer. A removal or an incompatible version found later is reported once, with the same way back.

## Mechanism

The client half of Remote Devices is a Rust crate, `openaide-remote-client`, in the App Server's workspace: reading an invite, spending it, keeping one connection to the paired App Server, and a loopback gateway. The gateway is what lets the shared Frontend stay unchanged: it gives the Desktop the same endpoint and token shape a same-machine App Server publishes, and carries each request over one stream. The token is created per process and stops at the gateway; the App Server trusts the connection's key and never sees it. The crate is proven against the real App Server edge in the App Server's tests.

The Desktop's key pair is its only pairing secret. It is kept in the operating system's credential store on macOS and Windows and in an owner-only file elsewhere. Paired computers are stored as public key and name in the bootstrap preferences.

While a paired computer is in use, the parts of the Desktop that assume a same-machine App Server step aside:

- Adding a Project browses folders through the App Server, as the Web App does, because the native picker lists this computer's folders.
- The runtime environment choice (Windows or WSL) is hidden; it belongs to this computer.
- The Desktop does not restore an App Server after sleep; the resumable session reopens the connection.
- A Desktop update replaces only the Desktop. It detaches the client and never asks the other computer's App Server to shut down.

## Considered Options

- **A Desktop that is only a remote client.** Rejected: a computer that can run agents should keep doing so, and switching back must not need a reinstall.
- **Using several computers at once.** Rejected for now: the Frontend renders one App Server's state, and merging Projects and Tasks of several is a product change of its own.
- **Applying the choice without a restart.** Rejected for now: the Frontend's session, caches, and shell capabilities are created once for one App Server. The restart is the mechanism the runtime environment already uses.
- **A second copy of the client in TypeScript inside the WebView.** Rejected: iroh has no WebView binding here, and the key must not live in web storage.

## Consequences

- The Desktop binary now contains iroh, which makes it larger.
- The pairing code format is read by two Rust implementations, the App Server's and the client crate's. The App Server's tests decode a code it issued with the client crate, which keeps them from drifting.
- Secrets a user enters for an Agent or MCP server are kept by the App Shell that entered them. Entered on a Desktop that uses another computer, they are available to that computer's App Server only while this Desktop is connected. Moving that storage to the App Server's computer is a separate decision.
- Removing the last Remote Device stops the App Server's endpoint. The endpoint now waits for the removal reason to be sent first, so the removed device reports its removal instead of an unreachable computer.
- An App Server that keeps running with nobody at its computer, and pairing it from a terminal, are not part of this decision.
