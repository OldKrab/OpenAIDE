# Prototyping

Prototypes answer a specific product or engineering question quickly. They are disposable working material, not an alternate path for shipping production code.

## Principles

- State the decision the prototype must enable before writing it.
- Reuse production components, styles, and realistic density whenever the question concerns an existing surface.
- Keep data in memory and mutations stubbed unless persistence or integration is the question.
- Build only enough behavior to evaluate the decision.
- Keep prototype implementations ignored by Git and delete them after recording the result.
- Rewrite the selected direction to production standards. Do not promote prototype code directly.

## UI Prototype Workspace

UI prototypes live under:

```text
packages/frontend/prototypes/<prototype-name>/prototype.tsx
```

The entire `packages/frontend/prototypes/` directory is ignored. Never force-add files from it. The committed harness under `packages/frontend/prototype-harness/` provides React, Vite hot reload, production fonts and styles, variant controls, and the Target route.

Create a prototype from the repository root:

```sh
npm run prototype:new -- live-activity
```

The generated module exports a typed definition with a question and variants. Keep only variants that test meaningfully different structures or interaction models. One variant is enough when the question is narrow.

### Reuse production code

Production components can be imported directly because the harness runs inside the frontend workspace:

```tsx
import { ChatActivityView } from "../../src/components/ChatActivityView";
import { definePrototype, PrototypeCanvas } from "../../prototype-harness/src/prototypeApi";
```

The harness already loads `tokens.css` and `app.css`. Prefer real components and local fixture data over reimplementing the interface. If a production component needs extensive product state, create a small prototype-local adapter or fixture builder. Commit a shared fixture only when it has durable test or development value outside the prototype.

### Run and iterate

The disposable Target owns prototype access. Start the hot-reloading server with:

```sh
npm run prototype:target -- live-activity
```

The command prints the Target path. When `OPENAIDE_WEB_PUBLIC_URL` is configured locally, it prints the complete browser URL. The Target web server authenticates the request and proxies `/prototype/*` HTTP and HMR WebSocket traffic to the loopback-only Vite server. Driver does not enable this proxy.

```text
Browser → Target authentication → Target web server → prototype Vite server
                                      └──────────────→ primary Web App
```

Keep the command running during review. Saving a prototype or imported production component updates the browser through Vite HMR; rebuilding the main frontend and restarting the App Server are unnecessary.

Before sharing a prototype:

1. Open the printed Target path in the browser.
2. Verify the intended variant and interactive state.
3. Confirm one edit appears through HMR.
4. Check the relevant desktop and narrow viewport.
5. Inspect browser console errors and visible overflow.
6. Give the reviewer a clickable Markdown link to the verified prototype route, including the selected `?variant=` when relevant.

Do not treat a bare Target path as the final handoff when a reviewer-facing Target origin is available. If `OPENAIDE_WEB_PUBLIC_URL` is not configured, resolve the active Target origin from the local runtime environment before responding; do not hard-code a private deployment domain in repository files. Do not share workspace paths, `/tmp` paths, internal Vite ports, or URLs that were not opened successfully.

### Variants

The harness reads `?variant=<key>` and supplies the standard bottom switcher. Use variants to compare different hierarchy, layout, or interaction approaches, not cosmetic color changes. Put additional prototype state in query parameters when a reviewer must share or reload a specific case.

### Finish and clean up

Record the selected direction and why in the relevant issue, ADR, task, or implementation commit. Then delete the ignored prototype:

```sh
npm run prototype:clean -- live-activity
```

The prototype server can be stopped when no review is active. A request made while it is stopped returns an explicit unavailable response instead of falling through to the primary application.

## Logic Prototypes

For a state model or algorithm that does not benefit from the UI harness, create an ignored throwaway program under `tmp/prototypes/<prototype-name>/`. Give it one command to run, print the complete relevant state after each action, avoid production persistence, and delete it after the result is recorded.

## Committed Boundaries

Only reusable infrastructure belongs in Git:

- Prototype harness and lifecycle scripts.
- General fixture or shell utilities with demonstrated reuse value.
- This workflow documentation.
- The durable decision produced by a prototype, when it belongs in an issue, ADR, test, or production implementation.

Prototype implementations, screenshots used only during review, transient data, and abandoned variants do not belong in Git.
