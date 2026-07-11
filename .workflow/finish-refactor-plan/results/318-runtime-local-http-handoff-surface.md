Packet ID: P318-runtime-local-http-handoff-surface
Status: completed

Objective:
Expose a real runtime handoff surface that lets VS Code obtain LocalHttp App
Server connection info through the shared Rust attach-or-launch logic rather
than reading endpoint files or duplicating discovery in TypeScript.

Changes:
- Added runtime protocol mode `app-server-handoff`.
- Added a focused binary module for handoff startup logic so `main.rs` remains
  below the production source-size limit.
- Handoff mode uses `AttachOrLaunchHandoff` to attach to an existing compatible
  LocalHttp endpoint or elect the current process as launcher.
- Elected launchers publish the LocalHttp endpoint, print bootstrap-compatible
  connection JSON, and keep the App Server Protocol edge running.
- Existing endpoint attach prints the same connection JSON and exits.
- Added `RuntimeProcess.startAppServerConnection()` in the VS Code shell. It
  spawns handoff mode and validates the returned connection info without
  reading endpoint records in TypeScript.
- Cached VS Code handoff connection info is cleared when the launched child
  exits.
- VS Code handoff parsing rejects non-loopback URLs, invalid paths, missing
  ports, and empty tokens.
- VS Code handoff stdout reads have a timeout and maximum line size.

Review:
- Bounded subagent review found shape-only LocalHttp validation and unbounded
  stdout read issues; both were fixed before commit with regression tests.

Verification:
- `cargo fmt --all --check`
- `cargo check -p openaide-runtime`
- `cargo test -p openaide-runtime --test runtime_contract app_server_handoff -- --nocapture`
- `npm run test --workspace openaide-vscode-extension -- src/runtime/processAppServer.test.ts src/runtime/process.test.ts`
- `npm run check --workspace openaide-vscode-extension`
- `npm run check`
- Changed production source files remain below the project size threshold.

Next:
Wire VS Code webview surfaces to request `RuntimeProcess.startAppServerConnection()`
and include the returned connection in bootstrap, while preserving responsive
fallback UI while handoff is pending or failed.
