# Baseline checks at 9b318c8

## Passed

- `npm run check`
  - Rust workspace check
  - generated App Server Protocol binding check
  - app-server-client build
  - app-shell-contracts typecheck
  - distribution import check
- app-server-client tests: 39 passed
- Web App tests: 15 passed
- VS Code extension tests: 70 passed
- App Server unit tests: 491 passed

## Failed

- Frontend tests: 4 failed, 541 passed.
  - `src/styles/app-css.test.ts`: image attachment token sizing contract no longer matches CSS.
  - `src/styles/app-css.test.ts`: opened attachment image preview sizing contract no longer matches CSS.
  - `src/components/NewTaskView.test.tsx`: prepared image attachment expectation still requires visible `pasted.png` text.
  - `src/components/NewTaskView.test.tsx`: submitting-state attachment expectation still requires visible `pasted.png` text.
- App Server runtime-contract tests: 2 failed, 37 passed.
  - `permission_requests_split_streamed_agent_text_runs`: received one merged message (`Need approval.After approval.`) instead of two text runs separated by the permission request.
  - `tool_call_updates_replace_existing_activity_by_identity`: projected activity retained `tool_call_1` instead of the update identity `tool_read_1` expected by the contract.

These are baseline failures, not regressions introduced by the audit workflow.
