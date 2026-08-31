# Windows updates may launch before Authenticode

Windows Desktop releases may use production in-app updates before SignPath
Foundation Authenticode signing is available. The Tauri updater Release Key
still authenticates the complete updater artifact, and the native updater still
enforces the canonical feed, immutable GitHub Release URL, digest, size,
revalidation, shutdown barrier, receipt, and explicit user-consent rules from
ADR-0055 and ADR-0056.

This is an explicit exception to ADR-0055's platform-signing activation gate.
It does not describe an unsigned update as publisher-signed and does not promise
that SmartScreen, Smart App Control, or enterprise policy will accept the newly
installed executable. The UI and download documentation keep that limitation
visible. Authenticode can be added later without changing the updater trust key
or feed contract.

During the alpha period, every published update-capable stable Windows release
is assumed storage-compatible with the newest stable Windows release. The feed
therefore routes every stable source containing an updater artifact directly to
the newest stable target. Prereleases form a separate channel. This supersedes
ADR-0055's both-platform atomic advance and ADR-0057's exact-edge compatibility
evidence requirements for this Windows-only alpha path. Before OpenAIDE permits
an irreversible durable-state migration, it must restore proven Update Edges or
introduce the snapshot and recovery design required by ADR-0057.

macOS production updates remain unavailable. Its safe bundle-replacement and
notarization limitations are not weakened by this Windows exception.

The release workflow signs the Windows NSIS installer with the protected
Desktop Update Release Key, publishes the installer and detached signature as
immutable release assets, and atomically regenerates the complete static feed
on GitHub Pages. A release is a source only after it contains its own updater
signature, so older manually installed releases never contact or trust the feed.
