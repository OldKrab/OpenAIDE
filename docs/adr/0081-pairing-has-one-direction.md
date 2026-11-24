# Pairing Has One Direction

Status: accepted; replaces the join request of ADR-0062

[ADR-0062](0062-remote-devices-are-app-server-clients.md) paired in two directions. In an invite, the computer shows a code. In a join request, the new device shows a code and the computer reads it. Both directions used the same mirrored pair of buttons, "Show code" and "Enter code", and the code alone did not say which side should show it or which side should enter it. Users entered a code on the wrong device. The Web App also had no way to use another computer at all. Pairing now has only the invite direction. Each side's action is named by what the user wants to do there.

## Choice

- **Only the invite.** The Computer that will be used shows a single-use Pairing Code. A device already using that Computer can show the code too. The device that will use the Computer enters or scans the code. A device shows codes only for the Computer it is using. To let a phone use a paired computer, the user first switches to that computer ([ADR-0070](0070-switching-computers-is-in-place-and-keeps-work-running.md)) and then adds the phone.
- **Two actions, each placed where its goal is looked for.** Both live in Settings, Remote, and are named by the goal ([ADR-0065](0065-settings-sections-are-named-by-the-users-goal.md)):
  - **Use another computer** is the last row of the Computers list and the last item of the computer switcher. It asks for a code.
  - **Add a device** is the action on the list of devices using the Computer in use. It shows a code for that Computer.
  - A new phone's first screen offers **Use another computer** next to working on the phone itself.
- **The direction is drawn.** Each pairing dialog opens with a picture of the connection: this device uses that computer. The end that is not known yet is drawn as an empty slot. Each dialog then names the other side's action as a path of controls:
  - the code dialog says: on the other device, Settings › Remote › Use another computer, or the first screen if the device is new;
  - the entry dialog says: get the code on the computer, Settings › Remote › Add a device.
- **A wrong code says what to do.** The device that reads a code checks it before spending it:
  - The code of the Computer this device runs itself: "This is <name>'s own code. Enter it on the device that should use <name>."
  - A join-request code from an older OpenAIDE: update the app on that device and use its new code.
  - An invite that was used, was cancelled, or has expired: make a new code on the computer.
  - Anything else: "This isn't an OpenAIDE code."
- **The long code stays.** A Pairing Code is `OAI1` followed by base32 of the App Server's public key and a single-use secret. That is about 81 characters, pasted on a computer and scanned as a QR code on a phone. Any future field, such as a relay or a token, needs a new version prefix.
- **One implementation of the code and the network.** A new Rust crate, `openaide-remote-core`, holds the Pairing Code format and the choice of network and relays. The App Server and `openaide-remote-client` both depend on it. Before this, the format had three implementations: two in Rust that a test kept in step ([ADR-0063](0063-desktop-can-use-another-computers-app-server.md)), and one in Kotlin in the Mobile App. The Mobile App moves onto the crate later through its native bindings.
- **The join request leaves the protocol.** `devices/previewJoinRequest` and `devices/approveJoinRequest` are removed, together with the Mobile App's join listener and the App Server's outgoing connection that answered a join request. Removing them is a major App Server Protocol version bump.

## Considered Options

- **Both directions with clearer labels.** Rejected. Every device that can use a computer can also enter or scan a code, so the second direction serves no case the first cannot. It costs a second code kind and a mirrored pair of buttons, and that is what users confused. [ADR-0063](0063-desktop-can-use-another-computers-app-server.md) had already left it out between two computers.
- **One Connect button that asks for the direction first.** Prototyped and rejected. "Connect" reads as an outgoing action, so a user who wants to let a device in does not look there, and every pairing pays the extra step. Products that keep both directions also name each by its goal: TeamViewer has "Allow Remote Control" and "Control Remote Computer", Chrome Remote Desktop has "Get Support" and "Give Support", and WhatsApp has "Link a device".
- **Sharing a paired computer from its row without switching to it.** Rejected. The code and the device list would belong to a computer other than the one on screen. Switching costs one step and keeps "Devices using <computer>" about the computer the user sees.
- **A short code.** Feasible, but deferred. The computer could publish its address under a short random locator through iroh's public pkarr relay. A password-authenticated key exchange, such as CPace or SPAKE2, would then turn about 10 characters into a verified key. OpenAIDE would run no service of its own, which answers the objection of [ADR-0064](0064-a-terminal-command-keeps-the-app-server-reachable.md). The costs are:
  - every pairing would depend on a second public service and on its record arriving in time;
  - a short secret can be guessed online, so the computer would have to limit attempts;
  - a key exchange protocol would join the code format.

  Pasting already covers computers and scanning covers phones.

## Consequences

- Every Remote Device must update together with the computers it uses. A device with the older protocol major is refused with the usual message to update.
- The Mobile App loses "Show my code". The `remote pair` command keeps printing an invite, and its text now names the other side's action.
- To recognize its own code, a device needs the public key of the App Server it runs. The App Server Protocol does not carry that key today. The change that implements the messages adds it.
