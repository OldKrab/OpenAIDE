# The Web App Can Use Another Computer

Status: accepted

The Web App could use only the App Server of the machine that serves it. A user with a Web App on one machine and OpenAIDE on another could not reach the second from the first. Now the Web App can use another Computer the way the Desktop App does ([ADR-0063](0063-desktop-can-use-another-computers-app-server.md), [ADR-0070](0070-switching-computers-is-in-place-and-keeps-work-running.md)): it pairs with a Pairing Code ([ADR-0081](0081-pairing-has-one-direction.md)) and switches in place.

## Choice

- **The Web App's server is the client.** The Web App's Node server holds the key pair and the list of paired Computers. They live in a folder that belongs to the Web App, under its state root, and only the owner can read that folder. The App Server's own state stays separate. The browser holds no key.
- **The Rust client is reused.** The Node server runs the gateway from `openaide-remote-client` as a child process. The gateway is a subcommand of the App Server binary that the Web App already ships. It publishes the same endpoint and token shape as a same-machine App Server, so the server's existing proxy to an App Server also carries traffic to a paired Computer.
- **Each browser chooses its Computer.** The Computer in use is chosen by each browser, not by the deployment. The server routes each browser's requests to the App Server that browser chose. When one browser switches, no other browser moves.
- **The Web App's own App Server keeps running** while a browser uses another Computer, as on the Desktop.
- **The other Computer sees one Remote Device**, named after the machine that serves the Web App, however many browsers use it.
- **Sharing works as in any App Shell.** A browser adds a device to the Computer it is using with **Add a device**. To share a different Computer, it switches to that Computer first.

## Mechanism

The browser's choice is a cookie on the Web App's origin. It decides which Computer a newly loaded page opens on. The page then addresses that Computer in all its App Server paths, so it keeps the Computer it opened on when another tab of the same browser switches. The tab that switches reloads, as the Desktop App's window does ([ADR-0070](0070-switching-computers-is-in-place-and-keeps-work-running.md)); other tabs move when they next load. The server never forwards the browser's cookies to an App Server.

## Considered Options

- **iroh in the browser through WebAssembly.** Rejected:
  - it can only use relays, never a direct path;
  - the key pair would live in browser storage;
  - every browser would be a separate Remote Device that has to be paired on its own.
- **`@number0/iroh` in the Node server.** Rejected: it would be a fourth implementation of the Pairing Code and the connection, after the App Server, the Rust client, and the Mobile App, which works against the shared core of ADR-0081.
- **Reusing the App Server's key pair as the client key.** Rejected: one key would play two roles, and the client's identity would belong to the App Server's state instead of the App Shell's, unlike the Desktop's key.
- **One Computer choice shared by every browser.** Rejected: one user's switch would move every other open browser to a different Computer under them.

## Consequences

- The deployment's access control now also protects every Computer the Web App is paired with. Anyone who can reach the Web App can use them. A deployment without a password or an authenticating proxy exposes them too.
- Removing the Web App on the other Computer cuts off every browser of that deployment at once.
- The gateway child process becomes part of the Web App server's lifecycle. Its start, exit, and restart are logged like the App Server's.
- The Web App provides the Computers capability that the Desktop and Mobile shells already provide. The shared Remote section and the switcher work unchanged.
