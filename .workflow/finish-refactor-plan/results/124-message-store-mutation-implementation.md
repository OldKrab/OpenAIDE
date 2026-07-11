# Message Store Mutation Split Implementation

## Summary

Implemented the accepted Message Store mutation split as a structural refactor
with no intended behavior changes.

## Code Changes

- Added `storage/message_store/mutations.rs` for:
  - `Store::finish_latest_running_activity`;
  - `Store::resolve_permission`;
  - `Store::cancel_pending_permissions`;
  - private `validate_permission_decision`.
- Kept `storage/message_store.rs` responsible for:
  - message file backup and restore;
  - append and upsert;
  - read and pagination;
  - message metadata writes;
  - message version helpers;
  - optional file read and restore helpers.
- Kept moved operations as inherent `Store` methods with unchanged signatures.
- Did not widen low-level helper visibility; the child module can call the
  parent module's private `Store` helpers directly.

## Behavior Preservation

The implementation preserves:

- message JSONL serialization;
- message metadata writes;
- message version behavior;
- pagination behavior;
- latest-running-activity scan and update semantics;
- permission resolution errors and decision validation text;
- pending permission cancellation behavior;
- storage file layout.

## File Size Check

Production Rust files after the split:

- `storage/message_store.rs`: 220 lines;
- `storage/message_store/mutations.rs`: 137 lines.

Both are below the 400-line production source file limit.

## Next Step

Record the doomsday-review result and integration verification, then commit the
slice.
