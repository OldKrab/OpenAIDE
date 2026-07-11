# P424 Storage Lifecycle Gap Audit

## Status

Completed.

## Audit Result

Storage concurrency is already protected at the store boundary:

- `Store::open` acquires `StorageOpenGuard`.
- `StorageOpenGuard` holds a per-state-root `storage-writer.lock`.
- clean and unclean shutdown markers are persisted under `.openaide-runtime`.
- store tests cover blocked second opens, clone-held locks, stale lock files,
  clean markers, unclean markers, and schema mismatch.

The concrete missing lifecycle boundary was reconnect-grace expiry at the
gateway layer. `ClientHub` could expire clients, and `AppLifecycle` could enter
draining, but `RpcGateway` had no operation that tied those together.

## Implementation

Added `RpcGateway::expire_client_after_reconnect_grace` and the corresponding
`SharedRpcGateway` method.

Behavior:

- expires only clients whose reconnect grace has actually elapsed;
- preserves reattached clients when stale expiry attempts arrive later;
- interrupts expired client-scoped server requests;
- transitions App Server lifecycle to draining when the expired client was the
  last initialized client.

Also exposed `ServerRequestRuntime::observe_client_expired` over the existing
broker behavior.

## Verification

Passed:

- `cargo fmt --all`
- `cargo fmt --all --check`
- `cargo test -p openaide-runtime protocol_edge::tests::last_client_expiry_after_reconnect_grace_starts_draining`
- `cargo test -p openaide-runtime protocol_edge::tests::reattached_client_is_not_expired_by_old_grace_timer`
- `cargo test -p openaide-runtime client_lifecycle::tests`
- `cargo check -p openaide-runtime`
- `git diff --check`

## Next Packet

P425 should wire a transport/runtime timer path to call
`SharedRpcGateway::expire_client_after_reconnect_grace` automatically after
transport loss.
