# Orchestration: Whole project audit and repair

## Execution Rules

- Keep the original objective intact.
- Ask for approval before risky, expensive, external, or destructive actions.
- Keep immediate blocking work local.
- Delegate only bounded, disjoint, materially useful packets.
- Integrate packet results before final verification.

## Branching Rules

- If baseline checks fail, route each failure to the packet that owns the production seam.
- If a candidate bug lacks a behavioral seam, first identify the nearest public protocol, storage, runtime, or rendered-UI interface; do not test private implementation details.
- If two packets propose overlapping edits, pause both edits and integrate the ownership decision centrally.
- If a change touches App Server/protocol/runtime/process management, use `npm run web:target:restart`; frontend-only changes use `npm run web:target`.
- If broad verification fails after a narrow green test, isolate whether the change exposed a pre-existing baseline failure before revising the fix.

## Packet Prompts

- Packet A owns correctness across the full repository and uses the strict-review correctness rubric. Audit first; no edits until findings are integrated.
- Packet B owns requirements and behavioral-test adequacy across the full repository and uses the strict-review requirements rubric. Audit first; no edits until findings are integrated.
- Packet C owns architecture and code quality across the full repository and uses the strict-review code-quality rubric plus deep-module vocabulary. Audit first; no edits until findings are integrated.
- Packet D is local to the primary agent and owns Target browser/runtime UX review without overlapping static packet audits.

## Completion Audit

- Coverage inventory has no unassigned hand-written production area.
- Every accepted fix has red/green evidence or a documented reason a regression test was not viable.
- Changed production files meet the repository size rule.
- Protocol bindings are current.
- Target is refreshed/restarted and verified without touching Driver.
- Final report distinguishes fixed findings from residual risks and lists exact verification results.
