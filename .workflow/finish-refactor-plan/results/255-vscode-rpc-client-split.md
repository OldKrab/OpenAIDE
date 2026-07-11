# VS Code RuntimeClient Split

## Contract

Split JSON-RPC internals out of `apps/vscode-extension/src/runtime/rpcClient.ts` while
preserving `RuntimeClient` as the stable public VS Code App Shell runtime client.

Ownership:

- `rpcClient.ts`: product convenience methods, pending request ownership, runtime
  process startup, notification listeners, host request handler registry, and public
  `RuntimeClient` lifecycle.
- `rpcClientTypes.ts`: shared listener, pending request, and host handler types.
- `rpcLineHandler.ts`: runtime line classification, notification dispatch, response
  resolution, and malformed-message logging.
- `rpcWire.ts`: JSON-RPC ids, messages, response envelopes, parsing, and id validation.
- `rpcHostRequests.ts`: Backend-initiated host request execution and response envelope
  construction.

Do not change runtime method names, request params shapes, timeout values, startup
concurrency behavior, pending rejection on runtime exit/dispose, notification dispatch,
host request error codes, host response sanitization, closed-stdin logging, App Server
Protocol semantics, runtime process ownership, VS Code shell capabilities, or
task/Agent/settings route behavior.

Focused tests:

- Existing `apps/vscode-extension/src/runtime/rpcClient.test.ts` remains the behavior
  suite for moved JSON-RPC and host-request behavior.
- `npm run check` covers TypeScript import and type-boundary safety.

## Implementation

Implemented the split by moving JSON-RPC wire parsing/id validation, runtime line
classification, shared client types, and host request response construction into focused
modules. `RuntimeClient` remains the public shell runtime client and still owns product
convenience methods, pending request storage, startup, notification listener sets, and
handler registration.

Production source sizes after split:

- `rpcClient.ts`: 279 lines.
- `rpcClientTypes.ts`: 12 lines.
- `rpcLineHandler.ts`: 51 lines.
- `rpcWire.ts`: 42 lines.
- `rpcHostRequests.ts`: 37 lines.

## Review

`$doomsday-review`:

- Correctness: no findings.
- Code quality: local pass found the first split left `rpcClient.ts` above the 300-line
  guideline, so the slice was refined by extracting `rpcClientTypes.ts` and
  `rpcLineHandler.ts`.
- Final local narrow review after that refinement found no findings.

## Verification

Focused checks already run:

- `npm --workspace openaide-vscode-extension test -- src/runtime/rpcClient.test.ts`:
  pass.
- `npm run check`: pass.

Final checks:

- `npm test`: pass.
- `git diff --check`: pass.
- `jq empty .workflow/finish-refactor-plan/state.json`: pass.
- Changed production source-size scan: largest split file is `rpcClient.ts` at 279 lines.

## Commit

This commit: `refactor: split vscode runtime rpc client`.

## Next

After this slice is committed, select the next compact refactor slice from the current
plan and file-size/boundary pressure.
