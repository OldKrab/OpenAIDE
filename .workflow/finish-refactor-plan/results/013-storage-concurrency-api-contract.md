# P02 Storage Concurrency API Contract

Completed: 2026-06-26T19:42:02+03:00

## Selected Slice

Backend storage model and concurrent access protection.

## Accepted Interfaces

- `Store::open` is the normal durable product storage entry point. It resolves the state
  root, obtains or receives a process-level writer guard, creates required directories,
  validates schema/open compatibility, records open runtime metadata, and returns
  structured outcomes or errors.
- `storage_runtime` owns process-safety mechanics: writer guard, clean/unclean markers,
  open classification, schema compatibility facts, and storage-runtime metadata.
- Product storage modules own durable product records. They do not open lock files,
  inspect runtime endpoint records, or decide recovery state.
- In-process mutation mutexes may remain, but only as single-process ordering. They do
  not replace the cross-process writer guard.

## First Implementation Scope

- Add a process-level writer guard to `Store::open`.
- Add clean/unclean open markers under the state root.
- Add a structured open/recovery classification for clean, unclean, locked, schema
  incompatible, and unrecoverable states.
- Keep current file-based task persistence and atomic write helpers.
- Do not build the full transaction/outbox system yet.

## Required Tests

- A second `Store::open` for the same state root is blocked while the first store lives.
- Dropping the first store releases the writer guard.
- `Store::open` classifies an unclean previous shutdown.
- A clean close writes a clean marker.
- Schema mismatch returns a structured error.
- Runtime endpoint records remain outside durable product storage.
- Existing Task storage tests pass without exposing lock-file details.

## Next

Proceed to `P03-implementation-slice`: implement the narrow storage concurrency/open
safety slice with focused tests.
