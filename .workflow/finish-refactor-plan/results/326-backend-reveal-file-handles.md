# P326 Backend Reveal File Handles

## Scope

Added the backend-owned opaque reveal-handle registry and typed
`shell/revealFile` request producer.

## Decisions

- Reveal handle ids are generated opaque ids and do not encode local paths.
- The registry keeps raw paths in Backend memory only.
- Protocol request params carry only `fileHandleId` and safe label.
- Relative local paths are rejected at registration.
- This slice does not make VS Code open files yet; the next slice must add a
  shell-private resolve/open path for `fileHandleId`.

## Verification

- `cargo fmt --all --check`
- `cargo test -p openaide-runtime shell_file_handles -- --nocapture`
- `cargo test -p openaide-runtime shell_reveal_file_request_uses_opaque_handle_params -- --nocapture`

## Next

Wire VS Code `shell/revealFile` handling through a shell-private App Server
file-handle resolve/open path so real files can be shown without exposing raw
paths through Frontend state.
