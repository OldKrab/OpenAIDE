# Message Store Mutation Split API Contract

## Decision

Split product-shaped message mutation operations out of
`storage/message_store.rs` into `storage/message_store/mutations.rs`.

This is a structural refactor only. Existing `Store` method signatures and
storage behavior must not change.

## Module Boundary

`storage/message_store.rs` remains the message log persistence module and owns:

- `MessageFilesBackup`;
- `Store::backup_message_files`;
- `Store::restore_message_files`;
- `Store::append_message`;
- `Store::upsert_message_by_identity`;
- `Store::tail_page`;
- `Store::page_before`;
- `Store::read_messages`;
- `Store::message_history_version`;
- low-level private helpers:
  - `write_messages`;
  - `write_meta`;
  - `page_from_slice`;
  - `next_message_version`;
  - `read_message_version`;
  - optional file read/restore helpers.

`storage/message_store/mutations.rs` owns existing-message rewrite operations:

- `Store::finish_latest_running_activity`;
- `Store::resolve_permission`;
- `Store::cancel_pending_permissions`;
- private `validate_permission_decision`.

The moved operations remain inherent `Store` methods with the same visibility,
names, parameters, return types, and errors.

## API Shape

`message_store.rs` declares:

```rust
mod mutations;
```

The child module implements methods directly on `Store`:

```rust
impl Store {
    pub fn finish_latest_running_activity(...);
    pub fn resolve_permission(...);
    pub fn cancel_pending_permissions(...);
}
```

The child module may call parent-module private helpers such as
`write_messages` and `write_meta` through `Store` without widening those helpers
unless Rust visibility requires a minimal `pub(super)` change.

No caller imports change. Existing call sites continue to call the same `Store`
methods.

## Behavior That Must Stay Unchanged

- Message JSONL serialization stays one stored message per line.
- Message metadata write behavior stays unchanged.
- Message version behavior stays unchanged.
- `finish_latest_running_activity` still:
  - scans messages from newest to oldest;
  - updates only the latest `ActivityStatus::Running` activity;
  - updates running tool and command steps inside that activity to the supplied
    status;
  - writes messages and metadata only when a change was made;
  - returns whether a change was made.
- `resolve_permission` still:
  - finds permission messages by `request_id`;
  - rejects already resolved permissions with
    `InvalidParams("permission already resolved")`;
  - rejects unknown option ids with `InvalidParams("option_id")`;
  - rejects unknown request ids with `InvalidParams("request_id")`;
  - validates option kind against the selected decision with the same error
    text;
  - records selected option and decision;
  - writes messages and metadata on success.
- `cancel_pending_permissions` still:
  - resolves every unresolved permission message as denied;
  - clears `selected_option`;
  - writes messages and metadata only when at least one permission changed;
  - returns whether a change was made.
- Public method visibility stays as it is today.

## Test Expectations

Existing storage, task mutation, turn, and runtime contract tests should remain
the behavioral proof for this slice. Run at least:

- `cargo test -p openaide-runtime storage::tests -- --nocapture`;
- `cargo test -p openaide-runtime tasks::mutation -- --nocapture`;
- `cargo test -p openaide-runtime`;
- `cargo fmt --all --check`;
- `npm run check`;
- `git diff --check`.

If moving the methods requires changing helper visibility, add no new behavior;
the review must verify that visibility is still no broader than needed.

## Rejected Directions

- Do not split backup/restore helpers in this slice.
- Do not introduce a message-store trait or repository abstraction.
- Do not change message file names, storage layout, pagination, or versioning.
- Do not change task mutation callers.
- Do not add new product behavior or recovery behavior.

## Next Step

Implement the mutation-method move, then run doomsday-review against storage
behavior preservation, helper visibility, and module isolation.
