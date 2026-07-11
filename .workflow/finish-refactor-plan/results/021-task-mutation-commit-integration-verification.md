# Task Mutation Commit Integration Verification

Date: 2026-06-26

Verification passed after implementation and review fixes:

- `cargo fmt --all --check`
- `cargo test -p openaide-runtime tasks::mutation -- --nocapture`
- `cargo test -p openaide-runtime --test runtime_contract task_list_revision_is_monotonic_across_completion_archive_and_restart -- --nocapture`
- `cargo test -p openaide-runtime --test runtime_contract task_delete_deletes_bound_native_session_when_supported -- --nocapture`
- `cargo test -p openaide-runtime`
- `npm run check`
- `npm test`
- `git diff --check`

Source-size check for changed production Rust files:

- `tasks/turns.rs`: 192 lines
- `tasks/turn_events.rs`: 336 lines
- `tasks/mutation.rs`: 275 lines
- `storage/message_store.rs`: 349 lines
- `tasks/service.rs`: 360 lines
- `tasks/transitions.rs`: 289 lines

Known existing debt outside this slice:

- `tasks/turn_lifecycle.rs` remains over the production file-size limit and is part of the documented unmigrated Task workflow surface for later refactor slices.

