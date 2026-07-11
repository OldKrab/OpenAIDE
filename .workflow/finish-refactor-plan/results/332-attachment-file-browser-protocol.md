# P332 Attachment File Browser Protocol

## Summary

- Added typed App Server Protocol methods:
  - `attachment/listRoots`
  - `attachment/listDirectory`
  - `attachment/createFileReference`
- Added opaque file browser ids and render-safe file browser protocol records.
- Extended `attachment_runtime` with Task-root listing, directory listing, opaque entry handles, and file-reference handle creation.
- Routed the new methods through `RpcGateway` into `TaskProductApi` via `AttachmentFileBrowserWorkflow`.
- Added stdio coverage for list roots -> list directory -> create handle -> `task/send` with the handle.
- Regenerated TypeScript protocol bindings.

## Verification

- `cargo fmt --all --check`
- `cargo test -p openaide-app-server-protocol attachment -- --nocapture`
- `cargo test -p openaide-runtime attachment_runtime -- --nocapture`
- `cargo test -p openaide-runtime attachment_file_browser_creates_handle_used_by_task_send -- --nocapture`
- `npm run protocol:generate`
- `npm run check`

## Next

- Continue A9 with attachment refresh/release and embedded candidate confirmation, then wire Frontend composer intents to the typed file browser methods.
