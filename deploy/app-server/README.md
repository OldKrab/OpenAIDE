# OpenAIDE App Server for Linux

Runs OpenAIDE on a Linux x64 computer that has no OpenAIDE window on it, such
as a server, so that a phone or the Desktop App on another computer can work on
it. Projects, Tasks, and Agents live on this computer; the paired device only
shows them.

No port is opened and no address has to be reachable from outside. The
connection is encrypted end to end and may pass through a public relay when the
two sides cannot reach each other directly.

## Install

The archive holds one self-contained program. Put it somewhere on your `PATH`:

```sh
tar -xzf openaide-app-server-linux-x64-VERSION.tar.gz
install -D openaide-app-server/openaide-app-server ~/.local/bin/openaide-app-server
```

Agents are installed and signed in separately, from the paired device's
Settings. The built-in Codex Agent needs Node.js, npm, and network access on
this computer the first time it is used.

## Start and pair

```sh
openaide-app-server remote --pair
```

This starts OpenAIDE and prints a pairing code, as text and as a QR code. The
code works once and for ten minutes.

- **Phone:** open OpenAIDE, choose to connect to a computer, and scan the QR
  code or paste the text.
- **Desktop App:** open **Settings → Desktop**, add a computer, and paste the
  text.

OpenAIDE stays reachable for as long as this command runs. Press Ctrl+C to
stop it; if Tasks are running it asks you to press again.

From a second terminal, while it runs:

```sh
openaide-app-server remote pair            # a code for another device
openaide-app-server remote devices         # the paired devices
openaide-app-server remote remove 2        # remove the second one in that list
```

## Keep it running

The command runs in the foreground and stops with its terminal. To keep it
across a closed session and a restart, hand it to your service manager. With
systemd, save this as `~/.config/systemd/user/openaide.service`:

```ini
[Unit]
Description=OpenAIDE App Server
After=network-online.target

[Service]
ExecStart=%h/.local/bin/openaide-app-server remote
Restart=on-failure

[Install]
WantedBy=default.target
```

Then enable it, and let your services run while you are logged out:

```sh
systemctl --user enable --now openaide.service
loginctl enable-linger "$USER"
```

Pair, list, and remove devices with the same commands as above.

## Data, update, removal

Tasks, settings, paired devices, and diagnostics are kept in
`${XDG_DATA_HOME:-~/.local/share}/openaide`. Agents work with the access of the
account that runs the command, so run it as the account whose projects they
should reach, not as root.

To update, stop the command, replace the program with the one from a newer
archive, and start it again. Paired devices stay paired. A device running a
much older or newer OpenAIDE may be asked to update before it can connect.

To remove OpenAIDE, stop the command and delete the program. Delete the data
folder as well to remove Tasks and pairings; project files are never touched.
