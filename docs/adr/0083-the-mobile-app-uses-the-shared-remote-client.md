# The Mobile App Uses the Shared Remote Client

Status: accepted

The Mobile App reached a paired Computer through its own Kotlin client on iroh's Android bindings. That client held the endpoint, pairing, the connection to the Computer in use, and the rules that keep a connection alive on a phone. `openaide-remote-client` held the same things for the Desktop App, and the Pairing Code had a third copy in Kotlin. [ADR-0081](0081-pairing-has-one-direction.md) moved the code into `openaide-remote-core` and left the Mobile App to follow.

## Choice

- **One client, in Rust.** `openaide-remote-client` holds a node that owns the endpoint, redeeming an invite, the connection to the Computer in use, and the phone's rules: the endpoint is replaced after the app returns from the background, each further dial attempt starts on a new endpoint, a switch's dial keeps the endpoint the connection in use rides on, and a connection that stops receiving is logged. The Mobile App reaches the node through UniFFI bindings in `openaide-rs/android-remote`, as it already reaches the SSH client ([ADR-0069](0069-ssh-computers-are-started-like-termux.md)).
- **The shell keeps what only it can.** Kotlin stores the key and the paired Computers with Android Keystore, reports the network Android sees and the app leaving and returning to the screen, writes the node's events to the phone's diagnostics log, and words every outcome for the user.
- **iroh's Android bindings are dropped.** The app carries iroh inside its own library, built from the same iroh release as the App Server.

## Considered Options

- **Keep the Kotlin client on iroh's Android bindings.** Rejected: the rules learned from phones would stay out of reach of the Desktop App and the Web App's gateway ([ADR-0082](0082-the-web-app-can-use-another-computer.md)), which share `openaide-remote-client`; redeeming an invite would keep a Kotlin copy; and the bindings shipped their own iroh release apart from the App Server's.
- **Expose iroh's endpoint to Kotlin and keep only the rules there.** Rejected: the rules decide when an endpoint is bound and retired, so they would cross the language boundary on every dial.

## Consequences

- The key stays valid across the move: iroh's Android bindings stored the Ed25519 seed, which the node reads as it is.
- The phone's diagnostics events take the node's names, such as `remote_connect_started` and `remote_connect_completed`.
- The Android build compiles two Rust libraries. The node's is about the size iroh's Android library was.
- The Desktop App and the Web App's gateway still use the older connection type in `openaide-remote-client`; moving them onto the node is left for later.
