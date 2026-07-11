# P329 Agent Identity Policy Module

## Summary

- Added `agent_identity` as the App Server-owned product policy module for Agent identity rules.
- Moved custom Agent id validation, generated custom ids, label/icon normalization, env-name validation, and default-Agent selection into that module.
- Rewired custom Agent catalog mutations and Agent collection snapshots to consume the identity module instead of owning those decisions locally.

## Verification

- `cargo fmt --all --check`
- `cargo test -p openaide-runtime agent_identity -- --nocapture`
- `cargo test -p openaide-runtime agent_collection -- --nocapture`
- `npm run check`

## Next

- Continue A8 by extracting history/recovery product policy from Task snapshot/workflow code, then close A8 if no remaining core product owner is only a placeholder.
