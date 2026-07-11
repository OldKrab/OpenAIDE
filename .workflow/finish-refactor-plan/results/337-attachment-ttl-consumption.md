# P337 Attachment TTL And Consumption

## Result

- Pre-send handles, embedded candidates, and file-browser entry handles now carry expiry timestamps.
- Attachment runtime prunes expired resources on public lifecycle operations.
- `task/send` checks existing idempotency receipts before resolving live handles, preserving successful retry behavior after handle consumption.
- Successful sends consume attachment handles after task commit, so handles cannot be reused as composer state.

## Verification

- `cargo fmt --all --check`
- `cargo check -p openaide-runtime`
- `cargo test -p openaide-runtime attachment_runtime -- --nocapture`
- `npm run check`
- `git diff --check`
- Production source size scan excluding tests, generated files, examples, and `node_modules`

## Remaining

- Live open/reveal routing for attachment handles.
