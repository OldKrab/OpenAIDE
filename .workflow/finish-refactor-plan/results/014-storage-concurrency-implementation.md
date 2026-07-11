# P03 Storage Concurrency Implementation

## Scope

Implemented the first narrow storage concurrency slice from the accepted storage
contract.

## Changes

- `Store::open` now returns a structured `StoreOpenError` and acquires a private
  process-level storage open guard before mutating product storage directories.
- `Store` clones now share an internal `Arc<StoreInner>`, so the writer guard is
  held until the last storage handle is dropped.
- Storage open writes a runtime open marker under `.openaide-runtime` and classifies
  the previous marker as clean or unclean.
- Clean shutdown explicitly writes the clean marker; plain drop preserves the open
  marker so the next startup classifies the previous run as unclean.
- Incompatible storage runtime marker schema returns a structured `StoreOpenError`.
- Storage runtime marker and lock mechanics live in `storage_runtime`; product
  `storage::Store` owns product directories and holds the private guard.
- Runtime contract tests that previously opened sidecar stores during live runtime
  ownership now read through the owned store or drop the old owner before restart.

## Verification

- `cargo fmt --all`
- `cargo test -p openaide-runtime storage::tests -- --nocapture`
- `cargo test -p openaide-runtime --test runtime_contract -- --nocapture`
- `cargo test -p openaide-runtime`
- `npm run check`
- `npm test`
- Source-size scan: touched production storage file is 150 lines; touched large
  runtime contract files are tests and exempt from the production source limit.

## Review Notes

- The implementation intentionally keeps current file persistence and atomic writes.
- It does not add the later transaction/outbox seam.
- It does not wire recovery classification into Task recovery UI yet.

## Next

Proceed to `P04-review-loop`: review module isolation, Store open API shape, writer
guard lifetime, and test adequacy before closing this slice.
