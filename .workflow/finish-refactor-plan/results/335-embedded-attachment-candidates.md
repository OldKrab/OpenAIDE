# P335 Embedded Attachment Candidates

## Result

- Added typed `attachment/createEmbeddedCandidate` and `attachment/confirmEmbedded` protocol methods.
- Added `AttachmentCandidateId`, candidate records, per-candidate confirmation errors, and generated TypeScript bindings.
- Runtime now creates embedded candidates from file-browser entries, validates UTF-8 text and size, batch-confirms candidates into sendable handles, and captures text at `task/send`.
- Failed or wrong-task confirmation does not consume candidates.

## Verification

- `cargo fmt --all --check`
- `cargo check -p openaide-runtime`
- `cargo test -p openaide-app-server-protocol attachment -- --nocapture`
- `cargo test -p openaide-app-server-protocol methods -- --nocapture`
- `cargo test -p openaide-runtime attachment_runtime -- --nocapture`
- `npm run protocol:generate`
- `npm run protocol:check`
- `npm run check --workspace @openaide/app-server-client`
- `npm run check`
- `git diff --check`

## Remaining

- Frontend file-browser composer UI for choosing reference vs embedded mode.
- Any richer attachment validation UI around candidate errors.
