# Frontend Standalone Dev Host Split API Contract

## Decision

Accept the Frontend Standalone Dev Host split.

The external seam remains:

```ts
standaloneBootstrap(): WebviewBootstrap | undefined
createStandaloneHost(): { postMessage(message: unknown): void } | undefined
```

`hostBridge.ts` remains the production caller. No other production module should
import the new implementation modules directly.

## Public Exports To Preserve

`services/devHost.ts` keeps exporting:

- `standaloneBootstrap`
- `createStandaloneHost`

No new public package entry points are added.

## Internal Modules

Create focused private modules under `packages/frontend/src/services/`:

- `devHostData.ts`
  - owns demo constants, workspace roots, demo Agents, Task summaries, Task
    snapshots, native sessions, config options, Settings snapshot, and demo
    tool-detail response data;
  - exports typed factory helpers for the host router only.

- `devHostBootstrap.ts`
  - owns standalone browser detection, path-to-surface mapping, new-task path
    detection, and the `WebviewBootstrap` construction.

- `devHostRouter.ts`
  - owns `WebviewToHostMessage` validation and routing;
  - converts incoming messages to demo `HostToWebviewMessage` responses;
  - owns browser route transitions for standalone preview surfaces;
  - receives a small injected output interface for posting messages and
    navigating/reloading, so routing is testable without direct global access.

`devHost.ts` becomes the public facade that wires browser globals to the
internal bootstrap and router modules.

If implementation finds a smaller split cleaner, module names may vary, but the
same responsibilities must remain separated and `devHost.ts` must remain the
only public import path.

## Behavior To Preserve

- `standaloneBootstrap()` returns `undefined` when `window.acquireVsCodeApi` is
  present or when `document.body.dataset.surface` is present.
- Standalone path mapping remains:
  - paths containing `navigation` -> `surface: "navigation"`;
  - paths containing `settings` -> `surface: "settings"`;
  - all other standalone paths -> `surface: "task"`.
- Standalone task id is `"demo_task"` only for task routes that are not
  `new-task`; new-task routes have no task id.
- Default standalone preferences remain
  `{ composer_submit_shortcut: "mod_enter" }`.
- Standalone host returns `undefined` when `window.acquireVsCodeApi` is present.
- Non-object or object-without-string-`type` messages are ignored.
- All handled message types preserve current response type, payload shape,
  request id metadata, append flags, snapshot intent metadata, and async
  dispatch timing.
- `surface.openTask`, `surface.openNewTask`, and `surface.openSettings` keep
  using `history.pushState` to `/task`, `/new-task`, and `/settings` followed
  by reload.
- Unknown messages are ignored.

## Boundary Rules

- Demo data modules must be pure and browser-free: no `window`, `document`,
  timers, host bridge imports, React, reducers, or App Server client imports.
- Bootstrap helpers may read browser path/body facts through injected values or
  through the facade only; they must not route host messages.
- Router modules may depend on demo data and typed App Shell contracts, but
  should use injected output/navigation functions instead of reaching into
  `window` directly.
- `devHost.ts` is the only module in this slice allowed to wire direct browser
  globals to the standalone host facade.
- Do not introduce fixed domains, ports, paths from local conversations, or
  environment-specific source constants.

## Tests

Add focused tests for public behavior through the facade or router helpers.

Required coverage:

- Standalone bootstrap is disabled when VS Code API or webview dataset surface
  is present.
- Standalone bootstrap maps task, new-task, navigation, and settings paths.
- Standalone host ignores invalid messages.
- Representative message routing preserves response metadata:
  - `task.list` archive flag and revision;
  - `agent.listSessions` request id/key and append fields;
  - `agent.configOptions` or `session.setConfigOption` options request key;
  - `task.snapshot` snapshot request id and intent;
  - `tool.detail` task/artifact ids.
- Surface navigation routes push the expected path and reload.

## Out Of Scope

- No real Web App shell transport.
- No App Server attach-or-launch behavior.
- No conversion from demo host to App Server Protocol.
- No UI redesign or demo content rewrite beyond moving factories.
