# 303 Runtime Restart Policy Boundary

## Scope

Resolve the remaining A6 "runtime restart policy" item by assigning it to the
right architectural owner.

## Finding

Runtime restart and process reuse cannot be cleaned up safely as an isolated A6
VS Code Extension edit. The VS Code shell still has live legacy runtime routes
for task, Agent session, diagnostics, developer settings, and health messages.
Flipping the single child process to App Server Protocol mode now would break
those routes, while keeping it in legacy mode means typed App Server Protocol
requests are only available through transitional fallback behavior.

## Decision

- A6 is product-decision cleanup: Agent definitions/status, Settings truth,
  Project identity, and task/product ownership move into App Server.
- Runtime restart, protocol-mode selection, endpoint reuse, stale endpoint
  cleanup, process sharing, and launch conflict handling belong to A7 shared
  attach-or-launch.
- A7 must replace the VS Code-specific child-process restart policy with shared
  launcher/client mechanics from the App Server client boundary.
- Until A7, VS Code keeps the transitional runtime process path.

## A7 Acceptance Notes

- App Shells must not each invent runtime restart policy.
- Shared attach-or-launch owns compatible endpoint discovery, validation,
  process launch, launch locking, stale cleanup, protocol-mode selection, and
  lifecycle/reconnect outcomes.
- VS Code should call the shared launcher instead of deciding restart/reuse from
  `RuntimeProcess.start()`.

## Verification

- Inspected current VS Code runtime call sites and confirmed live legacy routes
  still call `RuntimeClient` methods such as `task.list`, `task.create`,
  `session.prompt`, `agent.configOptions`, diagnostics, settings, and health.
- Inspected `RuntimeProcess` and confirmed it currently launches one stdio child
  without `OPENAIDE_RUNTIME_PROTOCOL=app-server-protocol`.
- `jq empty .workflow/finish-refactor-plan/state.json`
- `git diff --check`

## Next

Start A7 by grilling the shared App Server attach-or-launch API.
