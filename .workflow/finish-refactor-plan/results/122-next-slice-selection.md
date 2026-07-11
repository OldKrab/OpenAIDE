# Next Slice Selection: Message Store Mutation Split

## Selected Slice

Split activity and permission message mutation helpers out of
`storage/message_store.rs` into a focused child module.

Tentative module shape:

- `storage/message_store.rs`: message file backup/restore, append/upsert, read,
  pagination, low-level message/meta write helpers, and message version helpers.
- `storage/message_store/mutations.rs`: message-level mutation operations that
  rewrite existing chat history:
  - `finish_latest_running_activity`;
  - `resolve_permission`;
  - `cancel_pending_permissions`;
  - permission decision validation helper.

## Why This Slice

`storage/message_store.rs` is now one of the larger remaining backend files and
mixes two different responsibilities:

- generic message log storage mechanics: read, write, page, backup, restore,
  version metadata;
- product-shaped mutations inside normalized messages: finishing activities,
  resolving permission prompts, and canceling pending permission prompts.

The split gives storage a clearer boundary before recovery, attachments, and
history work grow the message model. Keeping the mutation methods on `Store`
preserves the current task mutation API while isolating the product-shaped
message rewrite logic from low-level persistence helpers.

## Intended Boundary

`storage/message_store.rs` should keep:

- `MessageFilesBackup`;
- `backup_message_files`;
- `restore_message_files`;
- `append_message`;
- `upsert_message_by_identity`;
- `tail_page`;
- `page_before`;
- `read_messages`;
- `message_history_version`;
- private low-level helpers:
  - `write_messages`;
  - `write_meta`;
  - `page_from_slice`;
  - `next_message_version`;
  - `read_message_version`;
  - optional file read/restore helpers.

`storage/message_store/mutations.rs` should own:

- `finish_latest_running_activity`;
- `resolve_permission`;
- `cancel_pending_permissions`;
- `validate_permission_decision`.

The moved methods should remain inherent `Store` methods with the same
visibility and signatures so callers do not change.

## Constraints

- No behavior changes.
- Keep message JSONL and metadata write behavior unchanged.
- Keep message version behavior unchanged.
- Keep pagination behavior unchanged.
- Keep activity finishing semantics unchanged, including updating the latest
  running activity and running tool/command steps only.
- Keep permission resolution errors unchanged:
  - already resolved permission;
  - missing option id;
  - missing request id;
  - decision/option-kind mismatch.
- Keep pending permission cancellation semantics unchanged.
- Keep production Rust source files under the 400-line limit.
- Do not introduce a broader message-store abstraction or change storage file
  layout.

## Main Risks To Grill

- Whether child module methods can cleanly call the parent module's private
  write helpers without widening their visibility.
- Whether this split should include backup/restore helpers or stay focused only
  on product-shaped message mutations.
- Whether tests should remain in `storage/tests.rs` or add a focused
  `message_store` test file if coverage is weak.

## Next Step

Grill and record the API contract for the Message Store mutation split.
