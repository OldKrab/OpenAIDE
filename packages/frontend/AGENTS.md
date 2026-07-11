# AGENTS.md

## Frontend Boundary

- Treat this package as the shared Frontend for OpenAIDE App Shells.
- Share product UI, rendering components, and presentation helpers by default.
- Do not fork Task Page, Task Navigation, Chat, Composer, or Settings behavior per App Shell.
- Shared Frontend must support both project-first navigation for Web/Desktop and task-first current-workspace navigation for VS Code through composition, not duplicated product behavior.
- Keep App Shell-specific UI small and explicit: inject shell chrome, placement, menus, routing adapters, and capability affordances through narrow composition points.
- Frontend code renders App Server-owned product state and sends user intent back to App Server; do not move task lifecycle, settings truth, runtime routing, stale response guards, or persistence decisions into Frontend state.
- Mutating user actions go through a central Frontend intent layer rather than direct protocol calls from arbitrary components; intent helpers own responsiveness classification, stable client request ids, pending or optimistic presentation, and Backend reconciliation.
- Direct App Server Protocol access is limited to bootstrap/connection, subscription/state ingestion, Backend-initiated request handling, and the central intent layer; rendering components consume derived state and call intent helpers instead of invoking protocol methods directly.
- Frontend app code must consume generated typed protocol bindings; do not use untyped method strings or `unknown` protocol payloads for Backend requests, events, responses, or Backend-initiated Frontend/App Shell requests.
- Frontend convenience helpers may wrap the generic typed `request(method, params, meta)` shape, but those helpers are not the Backend/Frontend seam and must not leak into transport or protocol definitions.
- Prefer simple props, small adapter components, and typed shell capability objects over global conditionals or shell-name checks.
- If a shell-specific branch starts changing product behavior, stop and move the decision to App Server or App Server Protocol.

## Verification

- For shared UI changes, verify at least one wide editor-like viewport and one narrow/mobile viewport.
- For App Shell-specific injection points, test the shared default path and the shell-specific override path.
