# App Server Project Collection Snapshots

Implemented the first App Server-owned Project collection slice.

## Implemented

- Added storage-backed Project collection snapshot projection from visible Task records.
- Included Project collections in `client/initialize` and `state/subscribe`.
- Added typed `ProjectCollectionUpdated` App Server events and generated TypeScript bindings.
- Published Project collection updates after Task create/update/discard paths that can change visible Project state.
- Routed Project collection events as safe cursor-advance metadata for all subscription scopes so hidden Project updates do not create false cursor gaps.
- Updated `@openaide/app-server-client` ingestion so Project subscriptions apply Project collection updates and other subscriptions advance unchanged.
- Split task publication helpers out of `protocol_edge/task_handlers.rs` to keep production file size under the project threshold.

## Review

- First review found stale Project subscriptions and misleading Project snapshot error text.
- Second review found missing TypeScript client ingestion and cursor gaps from hidden Project events.
- Final review found no remaining issues.

## Verification

- `cargo test -p openaide-runtime state_sync::tests::project_collection_update -- --nocapture`
- `cargo test -p openaide-runtime protocol_edge::stdio::tests::task_create_emits_project_collection_update_after_initialize -- --nocapture`
- `cargo test -p openaide-runtime protocol_edge::stdio::tests::task_discard_emits_project_collection_update_after_last_project_task -- --nocapture`
- `cargo test -p openaide-app-server-protocol -- --nocapture`
- `npm run protocol:check`
- `npm run build --workspace @openaide/app-server-client`
- `npm run test --workspace @openaide/app-server-client`
- `cargo fmt --all --check`
- `git diff --check`

Full `cargo test -p openaide-runtime -- --nocapture` passed once after this slice. A later full-suite run exposed an existing timing-sensitive Task product test failure that passed when rerun directly.

## Remaining A6 Work

- App Server-owned custom Agent mutation/settings storage.
- Deeper Project canonicalization APIs beyond the current storage-derived Project collection.
- Runtime restart/update policy ownership cleanup.
