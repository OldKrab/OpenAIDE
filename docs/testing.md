# Testing guide

## Test placement

- Put Rust test bodies in dedicated test files. Use integration tests by default; private unit tests use the adjacent `<module>_tests.rs` convention. Shared integration helpers live in `tests/common/mod.rs`.

## Behavior and verification

- For concurrent tests, establish ordering with synchronization primitives (barriers, channels, events, or latches) and await or join workers for completion. Do not use sleeps, polling, or short wall-clock deadlines as substitutes for synchronization. Use timing assertions only for an explicit timing contract, preferably with a controlled clock; a generous timeout may guard against deadlocks but must not establish ordering. When a test flakes, fix its synchronization before increasing its timeout.
- For a production behavior bug, make the closest real boundary test red, implement the fix, and rerun it. Model ACP chunks, updates, and replayed history rather than mocking away protocol semantics.
- Do not add or update tests whose only evidence is matching source text, literal CSS selectors or declarations, token values, pixel values, or other implementation constants. Test user-observable behavior at a real boundary; prove visual-only changes in the browser at the required viewports instead of encoding the current stylesheet in a test.
- For visual-only work, verify the affected interaction in the browser at relevant wide and narrow viewports. Shared UI changes require both; shell composition requires the default and override paths.
- Run the narrowest relevant repository check first, then broaden when a shared contract changes. Read the available scripts and tool configuration rather than copying commands into this guide.
