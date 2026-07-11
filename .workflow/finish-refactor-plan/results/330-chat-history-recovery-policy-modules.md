# P330 Chat History And Recovery Policy Modules

## Summary

- Added `chat_history::ChatHistoryPolicy` and moved the task snapshot chat tail limit out of snapshot plumbing.
- Added `task_recovery` policy for volatile runtime recovery decisions and the canonical restart interruption message.
- Rewired Task recovery transitions to consume the recovery plan instead of owning the policy inline.
- Marked A8 complete: Projects were already App Server-owned, Settings and Agent identity were completed in prior slices, and history/recovery policy now have explicit product modules.

## Verification

- `cargo fmt --all --check`
- `cargo test -p openaide-runtime chat_history -- --nocapture`
- `cargo test -p openaide-runtime task_recovery -- --nocapture`
- `cargo test -p openaide-runtime recover -- --nocapture`
- `npm run check`

## Next

- Start A9: App Server-owned attachment runtime and file browser lifecycle.
