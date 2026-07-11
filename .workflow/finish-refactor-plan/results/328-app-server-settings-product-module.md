# P328 App Server Settings Product Module

## Summary

- Added `settings::SettingsCatalog` as the App Server-owned source for renderable Settings sections.
- Wired Settings snapshots into `SnapshotBuilder` so `client/initialize` includes Settings and `state/subscribe(Settings)` returns filtered Settings snapshots.
- Added focused Rust tests for Settings defaults, section filtering, initialize snapshots, and Settings subscriptions.

## Verification

- `cargo fmt --all --check`
- `cargo test -p openaide-runtime settings -- --nocapture`
- `npm run check`

## Next

- Continue A8 by moving Agent identity/catalog ownership behind a clearer product-facing module boundary instead of keeping identity rules embedded in runtime registry/product API code.
