# P331 Attachment Runtime Send Validation

## Summary

- Added `attachment_runtime` as the App Server-owned in-memory pre-send attachment handle boundary.
- Runtime resolves handles by Task, rejects unknown/wrong-task/duplicate handles, keeps raw paths only in memory for Agent delivery, and returns safe chat metadata without paths.
- Wired `TaskProductApi` to validate attachment handles during `task/send`.
- Extended send receipts with ordered attachment handle fingerprints so idempotent retries reject changed attachment sets.
- `task/send` now commits safe attachment metadata to Chat snapshots and passes raw file references only to Agent turn delivery.

## Verification

- `cargo fmt --all --check`
- `cargo test -p openaide-runtime attachment_runtime -- --nocapture`
- `cargo test -p openaide-runtime send_commits_valid_attachment_handles_as_safe_chat_metadata -- --nocapture`
- `cargo check -p openaide-runtime`
- `npm run check`

## Next

- Add protocol and App Server methods for file browser roots/listing and pre-send handle creation so shells can create real handles without test hooks.
