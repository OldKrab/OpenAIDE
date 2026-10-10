# Remote Devices Are App Server Clients

Status: accepted

An App Shell on another machine, such as the Mobile App on a phone, connects to the App Server as a regular client. It ships its own Frontend, speaks the App Server Protocol, and is trusted by a key pair instead of an account. OpenAIDE runs no sign-in service and stores no password: a Remote Device is trusted because a device that was already trusted paired it.

## Trust

App Shells started by the same user on the App Server's machine keep the existing proof: the endpoint record and its per-process token. A Remote Device proves itself with a key pair it generates and never shares. The App Server keeps its own key pair and a durable list of trusted device keys in its state root. Every trusted client is equal and has full access; any client can remove any Remote Device, and removal is permanent until that device is paired again. A Web App reached through a browser stays an ordinary same-machine client, and protecting the path from the browser to it remains that deployment's concern.

Pairing has two directions, both carrying a short plain-text Pairing Code that is also drawn as a QR code:

- **Invite**: a trusted client shows a code holding the App Server key and a single-use secret. The code is valid only while it is shown. The new device connects to that key and presents the secret.
- **Join request**: the new device shows a code holding its own key and its self-reported name. A trusted client reads it, shows which device will be added to which App Server, and confirms. The App Server then connects to the new device and tells it which App Server trusts it.

A paired device works immediately. Every connected client is told about it and can remove it. The name and model a device reports are labels for the user, not evidence: nothing can prove a device runs an unmodified OpenAIDE build, so trust rests on the pairing act alone.

## Transport

Remote Devices connect over [iroh](https://www.iroh.computer): each end is addressed by its public key, the connection is encrypted and authenticated end to end with those keys, and a direct path is used when the networks allow it. When they do not, traffic passes through a relay that sees only ciphertext and the two public keys. OpenAIDE uses the public relays operated by the iroh project and runs none of its own. The App Server contacts a relay only while remote access is in use: from the first Pairing Code until the last Remote Device is removed. The first use shows a notice that names the public relays and what they can observe.

A Remote Device opens one stream per request. The App Server checks the connection's key against the trusted list and then passes the stream to the same handlers that serve same-machine App Shells, so the resumable session of [ADR-0026](0026-resumable-http-rpc-session.md) carries a Remote Device across network changes without a second recovery mechanism. A Remote Device never receives the local token.

## Protocol Version

The App Server Protocol version is `major.minor` and is separate from the product version. A minor bump adds methods, fields, or events; a major bump is any change an older client cannot ignore. A client is compatible when its major equals the App Server's and its minor is not greater. Clients ignore fields and events they do not know. A client that is too new is refused with a message to update OpenAIDE on the computer. A repository check fails when the generated protocol bindings differ from the last release without a version bump.

Same-machine App Shells additionally keep the exact product-version match they already require, because they are installed and updated together with their App Server.

## Considered Options

Loading the Frontend from the computer into a mobile web view, protected by a username and password, was the previous design and is replaced: it required a reachable address, a reverse proxy, and credentials, and made the phone a browser of one Web App rather than a client of the App Server.

An OpenAIDE-operated account and broker service was rejected because the product has no server-side identity and should not need one to connect two devices the user already owns.

A VPN or tunnel the user configures was rejected as the default because it moves the whole setup burden to the user. It remains possible for a Web App deployment.

Running OpenAIDE's own relays was rejected for now: the public relays are operated at larger scale than the project could match, and no relay can read the traffic. A setting for a custom relay can be added without changing this decision.

Full semantic versioning of the protocol was rejected because a patch number carries no compatibility meaning for a wire protocol.

## Consequences

A measured mobile connection commonly stays on a relay, because carrier networks often block direct paths. A relayed path sustains roughly 1 MB/s and its round-trip time rises sharply during a bulk transfer, so interactive traffic must not queue behind large transfers. The iroh library adds about 14 MB to the Android app per CPU architecture.

The App Server still exits when its last same-machine client leaves, so a Remote Device can reach it only while an App Shell on the computer keeps it running. When an App Server may outlive its local clients is a separate decision.

Because a Remote Device ships its own Frontend, the protocol's compatibility rules become a release obligation: a breaking change now strands every device that has not updated. Notifications while the Mobile App is closed need either a long-lived connection held by a foreground service or a push service, and are not decided here.
