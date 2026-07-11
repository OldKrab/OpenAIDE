# P422 Fast Workflow Sync

## Status

Completed.

## What Changed

Updated workflow state so the active cursor no longer points at already-finished
internal split slices.

Confirmed from `docs/refactor-plan.md` and source layout that these documented
next slices are already implemented:

- Storage message mutation split.
- Task turn prompt split.
- Task turn event-sink split.
- Agent tool-details sanitizer split.
- ACP options session client split.
- ACP session opening split.
- Frontend/controller/component split backlog currently recorded as implemented.
- Backend transport/protocol/client/generator/test-layout split backlog
  currently recorded as implemented.
- Agent prompt-content split.
- ACP active-session registry split.
- ACP probe/auth runner split.
- ACP session termination split.

## Workflow Change

P423 is now an audit selector packet:

- inspect current code and the refactor plan;
- identify the first real unimplemented architecture or cleanup gap;
- implement that gap directly if it is concrete;
- prove completion if no real plan work remains.

## Verification

No product code changed in this packet.

Checked:

- `git status --short`
- `docs/refactor-plan.md` relevant completed-slice sections
- module presence for storage message mutations, prompt content, and ACP
  session termination
