# A Computer Can Use Its Own Relay

Status: accepted; amends the relay choice of [ADR-0062](0062-remote-devices-are-app-server-clients.md)

A Remote Device that cannot reach a computer directly goes through a relay. ADR-0062 uses the free public relays the iroh project runs. On a mobile network, where the path usually stays relayed, those relays measured about 45 KB/s ([ADR-0084](0084-chat-images-load-as-previews.md)). A relay that someone runs for a few people is much faster, but it has to turn everyone else away, so it admits each person by a password. This decision lets a computer use such a relay, and lets anyone run one with a single command. The [setup guide](../own-relay.md) is the user-facing version.

## Choice

- **Relay key.** A relay key is one line: the relay's address with a password, `https://relay.example.com/?token=<password>`. Each person gets their own key, so one person's access can be revoked without affecting anyone else. Every key may use the relay. A key made with the manage permission may also list, add, and revoke keys.
- **`openaide-relay`.** OpenAIDE ships a small Linux program for x64 and ARM64 that runs iroh's relay with per-person keys. `sudo ./openaide-relay setup` installs it as a systemd service, gets an `https` certificate from Let's Encrypt, and prints the first key, which can manage. The certificate may be for a DNS name or for the server's bare IP address, so no domain is needed. A bare IP address gets Let's Encrypt's six-day certificates, renewed automatically. When port 443 is taken, `--port` and `--cert-dir` serve on another port with certificates that something else renews. The server's own commands `add`, `show`, `revoke`, and `list` manage keys without a restart.
- **Management API.** The relay serves `/openaide/relay/v1` on the same `https` port, with the key's password as a bearer token: `GET me`, `GET people`, `POST people?name=&canManage=`, and `DELETE people?name=`. Revoking cuts that key's relay connections at once. The last key that can manage cannot be revoked. There are no rate limits.
- **Setting the relay.** A client sets or clears the relay key with `devices/setRelay`, added in App Server Protocol 4.2, from Settings › Remote. The terminal command `openaide-app-server remote relay set` does the same and reads the key on standard input, so that the password stays out of process arguments and shell history. The App Server first asks the relay who the key belongs to, and refuses a key the relay does not know. A relay without the management API, such as a stock iroh-relay with a shared password, cannot be asked, so its key is kept unchecked.
- **A manage key manages only.** Given a key that can manage, the App Server adds an ordinary key named after the computer. It relays and shows Pairing Codes with that key, and keeps the manage key only for managing. A device therefore never receives a key that can add or revoke people. When the computer later changes relay, it revokes the key it made for itself.
- **My relay.** With a manage key, Settings › Remote lists the people the relay admits. It can add a person, which takes a name and gives back their key as text and as a QR code, with "Can add and remove people" off by default. It can also revoke a person. Any device of that computer can do this, through `devices/relayPeople`, `devices/relayAddPerson`, and `devices/relayRevokePerson`. The App Server refuses to revoke the computer's own key.
- **Refused.** The device collection snapshot carries `ownRelay`: the relay's host, whether this computer can manage it, and whether the relay accepts this computer's key. The App Server asks again when the edge binds and after a relay check fails. A revoked computer's Settings then says "The relay no longer accepts this computer" and offers to take a new key or to go back to the free relays.
- **Binding.** With an Own Relay, the network edge binds with that relay alone and uses no public address lookup, so nothing about the computer is published. The relay must be reachable over `https`, because the key carries a password.
- **Pairing Code.** A computer with an Own Relay shows a Pairing Code that starts with `OAI2` and carries the relay key along with the computer's key and the secret. A code without one keeps the `OAI1` form, byte for byte. A device saves the relay key with the computer, and both pairing and every later connection dial the computer through that relay with that password. A client from before this change refuses an `OAI2` code as malformed.
- **Changing the relay.** Setting, replacing, or clearing the relay removes every paired device and discards the computer's identity, as removing the last device does. Each device must be paired again. A device knows the relay only from its Pairing Code, so a device paired before the change could not find the computer afterwards. Clients warn before the change.
- **No fallback.** If the Own Relay is down, devices outside the computer's network cannot reach it until the relay returns or the relay is cleared.

## Considered Options

Publishing the relay to the public address lookup, so that a code would not need to carry it, was rejected. A device would still need the password, and the lookup would show who uses the relay.

One password shared by everyone was rejected because it cannot be revoked for one person.

Installing the App Server on the relay server, and using its accounts, was rejected. A relay needs no projects, Agents, or task state, and a small single-purpose program is easier to install on a cheap server.

Requiring a DNS name was rejected. Let's Encrypt issues certificates for IP addresses, so a server with a public IP address is enough.

A separate owner role above managers was rejected. Every manager may revoke any key, which is simpler to explain; the only rule is that the last manager stays.

An environment variable was rejected because a setting that clients can see and change is what lets a phone user switch relays.

Falling back to the public relays when the Own Relay fails is deferred. It needs a second address in the code and a rule for when to switch.

## Consequences

A relay key is a secret only in the sense that it grants use of a relay. It does not grant use of any computer: that still needs a paired key. A manage key also grants adding and revoking people, so the App Server keeps it apart from the key it relays with and never puts it in a Pairing Code.

The App Server keeps both keys in its private trust file. The Web App keeps a computer's relay key in its private computers folder and never sends it to a browser. Desktop keeps it in its preferences file, beside the list of computers. The Android app keeps it in its private preferences, encrypted with Android Keystore like its device key, and removes it when it forgets the computer. The relay server keeps its keys in a file only its service user can read.

Logs record whether an Own Relay is in use and the outcome of each management call. They never record the host, a key, or a person's name. A relay's `Debug` output shows only its host.

A client keeps one endpoint on the public network for all of its computers, and it adds a computer's Own Relay to that endpoint before dialing. That endpoint may pick the Own Relay as its home relay, and it then publishes the relay's address, without the password, under the device's key.

Management calls wait on the network, so the App Server runs them outside the protocol lock, and a slow relay does not stall other clients.
