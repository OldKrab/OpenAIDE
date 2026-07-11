# P16 Next Slice Selection

Completed: 2026-06-27T02:40:50+03:00

## Selected Slice

Move the inline ACP runtime tests out of `agent/acp.rs` into a separate Rust test
submodule.

## Why This Slice

- `agent/acp.rs` is mostly tests around a small `AcpAgentRuntime` facade.
- The project has a hard rule that Rust tests should live in separate files where
  practical.
- This makes the production ACP facade easier to review before deeper ACP runtime
  refactors.

## Scope

- Replace the inline `#[cfg(test)] mod tests { ... }` in `agent/acp.rs` with an
  external `#[cfg(test)] mod tests;`.
- Move the existing test body to `agent/acp/tests.rs`.
- Preserve test names, helpers, behavior, and coverage.
- Do not change ACP runtime behavior, protocol mapping, Agent lifecycle, or public
  exports.

## Main Risk

The move is mechanically simple but easy to get wrong through Rust module visibility or
relative import changes. Review should focus on accidental behavior edits and whether
the test module still exercises the same surfaces.
