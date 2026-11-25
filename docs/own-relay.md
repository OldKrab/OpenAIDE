# Your Own Relay

When your phone can't reach your computer directly, for example on mobile data, the connection goes through a relay. By default OpenAIDE uses free public relays, which are shared by many people and can be slow. A relay of your own is much faster. It runs on any Linux server with a public IP address, such as a small rented VPS or a home machine whose router forwards the ports. You don't need a domain name.

The design is recorded in [ADR-0085](adr/0085-a-computer-can-use-its-own-relay.md).

## 1. Install the relay on the server

On the server, download the program for its processor, then run setup:

```sh
curl -LO https://github.com/OldKrab/OpenAIDE/releases/latest/download/openaide-relay-linux-x64.tar.gz
tar -xzf openaide-relay-linux-x64.tar.gz
sudo ./openaide-relay setup
```

On an ARM server, such as a Raspberry Pi or an ARM VPS, use `openaide-relay-linux-arm64.tar.gz` instead.

Setup installs the relay as a service that starts with the server and gets a free `https` certificate from Let's Encrypt. Then it prints your first **relay key**, a line like `https://203.0.113.7/?token=…`. Keep it private: anyone who has it can use your relay and add people to it.

The server must accept connections on TCP port 443 and UDP port 7842. On a home machine, forward these ports on your router.

If you have a domain name pointing at the server, `sudo ./openaide-relay setup --name relay.example.com` uses it instead of the IP address.

If port 443 is already in use on the server, choose another port and point setup at certificate files that something else, such as Certbot, keeps up to date:

```sh
sudo ./openaide-relay setup --name relay.example.com --port 4443 --cert-dir /etc/letsencrypt/live/relay.example.com
```

The relay runs as its own user, `openaide-relay`, which must be able to read those files. Certbot's files are readable only by root by default; setup tells you if the relay can't read them.

## 2. Use it in OpenAIDE

On the computer your phone connects to, open **Settings › Remote**, choose to use your own relay, and paste the relay key.

Changing the relay removes every paired device, so pair your phone again afterwards with **Add a device**. The new Pairing Code tells the phone which relay to use.

Without the desktop app, run `openaide-app-server remote relay set` on that computer, paste the key, and press Enter.

## 3. Let other people use it

Because your first key can add and remove people, Settings › Remote shows **My relay** with everyone who has a key.

- **Add person**: enter a name and you get their own relay key, as text and as a QR code. Give it to them; they paste it into Settings › Remote on their computer. Turn on "Can add and remove people" only for people you trust to manage the relay.
- **Revoke**: their computers stop using your relay at once. Each person has their own key, so nobody else is affected.

On the server itself, `sudo openaide-relay list`, `add <name>`, `show <name>`, and `revoke <name>` do the same.

## If something goes wrong

- **"The relay doesn't accept this key"**: the key was mistyped or revoked. Ask whoever runs the relay for a new one.
- **"OpenAIDE couldn't reach the relay"**: check that the server is on, that `sudo systemctl status openaide-relay` shows it running, and that the ports above are open.
- **"The relay no longer accepts this computer"**: someone revoked this computer's key. Enter a new key, or go back to the free relays, from Settings › Remote.
