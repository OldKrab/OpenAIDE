# P333 Attachment Refresh And Release

## Summary

- Added typed App Server Protocol methods:
  - `attachment/refreshHandles`
  - `attachment/releaseHandles`
- Added backend runtime refresh and release behavior for pre-send attachment handles.
- Routed refresh/release through `AttachmentFileBrowserWorkflow` and `RpcGateway`.
- Extended stdio attachment coverage to refresh a created handle, send with it, and release it.
- Regenerated TypeScript protocol bindings.
- Split file-browser runtime code into `attachment_runtime/file_browser.rs` to keep production files under the source-size rule.

## Verification

- `cargo fmt --all --check`
- `cargo test -p openaide-app-server-protocol attachment -- --nocapture`
- `cargo test -p openaide-runtime attachment_runtime -- --nocapture`
- `cargo test -p openaide-runtime attachment_file_browser_creates_handle_used_by_task_send -- --nocapture`
- `npm run protocol:generate`
- `npm run check`

## Next

- Continue A9 with embedded snapshot candidates and confirmation, then Frontend composer integration.
