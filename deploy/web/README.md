# OpenAIDE Web App

A single-user OpenAIDE server for Linux x64. It serves the OpenAIDE interface to
a browser and runs Agents on this machine, with this user's files, tools, and
Agent logins.

Anyone who can reach the server can run commands as this user. It listens on
loopback by default and has no sign-in of its own beyond an optional shared
password; put an authenticating HTTPS proxy in front before reaching it from
another device.

## Requirements

- Linux x64 with a systemd user session
- Node.js 24 or newer on `PATH`
- The Agents you intend to use, installed and signed in for this user

## Install

```sh
mkdir -p ~/.local/opt ~/.config/openaide-web ~/.config/systemd/user
tar -xzf openaide-web-linux-x64-VERSION.tar.gz -C ~/.local/opt
cp ~/.local/opt/openaide-web/openaide-web.service ~/.config/systemd/user/
echo "PATH=$PATH" > ~/.config/openaide-web/env
systemctl --user daemon-reload
systemctl --user enable --now openaide-web
```

Open `http://127.0.0.1:5474`. To keep the server running while logged out, run
`loginctl enable-linger "$USER"` once.

The service file assumes `~/.local/opt/openaide-web`. Edit its `ExecStart` line
when unpacking elsewhere. To try the server without installing a service, run
`~/.local/opt/openaide-web/bin/openaide-web`.

## Settings

Add settings to `~/.config/openaide-web/env`, one `NAME=value` per line, then
run `systemctl --user restart openaide-web`.

| Setting | Default | Purpose |
| --- | --- | --- |
| `OPENAIDE_WEB_PORT` | `5474` | Listening port |
| `OPENAIDE_WEB_HOST` | `127.0.0.1` | Listening address |
| `OPENAIDE_WEB_ALLOWED_HOSTS` | none | Comma-separated public hostnames accepted in the `Host` header; loopback names are always accepted |
| `OPENAIDE_WEB_UPSTREAM_AUTH` | unset | Set to `1` to listen beyond loopback when a proxy authenticates every request |
| `OPENAIDE_WEB_PASSWORD` | unset | Enables the built-in HTTP Basic password; `OPENAIDE_WEB_USERNAME` defaults to `demo` |
| `OPENAIDE_WEB_PROJECT_ROOTS` | none | Folders offered as Projects on first start, separated by `:` |
| `OPENAIDE_WEB_TRANSPORT` | `webSocket` | Set to `http` on a network path that rejects WebSocket upgrades |

The server refuses to start on a non-loopback address unless
`OPENAIDE_WEB_PASSWORD` or `OPENAIDE_WEB_UPSTREAM_AUTH=1` is set.

## Remote access

Keep the server on loopback and publish it through a reverse proxy that
terminates TLS and authenticates every request, including WebSocket upgrades.
The proxy must:

- forward the original `Host` header and `X-Forwarded-Proto: https`;
- pass WebSocket upgrades for `/__openaide-app-server/`;
- leave long-lived connections open (no short read timeout on that path).

List the public hostname in `OPENAIDE_WEB_ALLOWED_HOSTS`.

Caddy forwards `Host`, `X-Forwarded-Proto`, and WebSocket upgrades by default:

```caddyfile
openaide.example.com {
    # Add the proxy's authentication directive here, for example forward_auth.
    reverse_proxy 127.0.0.1:5474
}
```

nginx needs the upgrade headers stated:

```nginx
location / {
    # Add the proxy's authentication directive here, for example auth_request.
    proxy_pass http://127.0.0.1:5474;
    proxy_http_version 1.1;
    proxy_set_header Host $host;
    proxy_set_header X-Forwarded-Proto $scheme;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_read_timeout 1h;
    client_max_body_size 0;
}
```

## Data and logs

Tasks, settings, and diagnostics live in `~/.local/share/openaide-web`
(`$XDG_DATA_HOME/openaide-web` when that variable is set), outside the install
directory. Back up that directory to keep Task history.

```sh
journalctl --user -u openaide-web -f
```

## Upgrade

```sh
systemctl --user stop openaide-web
rm -rf ~/.local/opt/openaide-web
tar -xzf openaide-web-linux-x64-NEW_VERSION.tar.gz -C ~/.local/opt
systemctl --user start openaide-web
```

The installed version is the `version` field of
`~/.local/opt/openaide-web/package.json`. Prerelease versions may change stored
data without migration support; back up the data directory before installing one.

## Uninstall

```sh
systemctl --user disable --now openaide-web
rm -rf ~/.local/opt/openaide-web ~/.config/systemd/user/openaide-web.service
```

Uninstalling leaves `~/.local/share/openaide-web` and `~/.config/openaide-web`
in place.
