These files are an ADR-0046 schema 2 Subagent history as the public Store wrote
it before ADR-0076: an empty snapshot and a journal with a message frame, a text
append frame, a second message frame, and a plan frame. They were captured from
`storage/subagents_tests.rs` helpers at `c7710d79` and are stored as bytes so the
migration to Chat streams is tested against the released format.
