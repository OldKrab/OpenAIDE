# P06 Next Slice Selection

Completed: 2026-06-26T19:40:42+03:00

## Selected Slice

Backend storage model and concurrent access protection.

## Why This Slice

- It is next in the module grill queue after process lifecycle and state roots.
- The current storage layer is file-based and uses in-process mutexes in Task service
  code, but it does not yet have an accepted cross-process writer guard integrated into
  `Store::open`.
- The process lifecycle slice added primitives for runtime locks and recovery
  classification, but storage still needs a deeper interface for opening, transactions,
  commit/outbox facts, clean/unclean markers, and schema/recovery errors.

## Next

Proceed to `P02-api-grill-next-slice`: grill the storage interface contract before
implementing process-level store guards or changing Task persistence.
