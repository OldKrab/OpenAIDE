# Chat virtualization library selection

Research date: 2026-08-05

## Question

Which React virtualizer best fits OpenAIDE's long, variable-height Chat timeline while preserving pinned-bottom streaming, history prepends, per-task restoration, text selection, and animated tool-group disclosure?

The answer is **`@tanstack/react-virtual`**, starting with React adapter `3.14.9` and its `@tanstack/virtual-core` `3.17.7` dependency. It is the strongest current fit because it now exposes the chat-specific geometry contracts OpenAIDE otherwise has to maintain itself: end anchoring, conditional append following, stable-key prepend anchoring, growing-last-row retention, dynamic measurement, and snapshot restoration.

`virtua` is the runner-up. Its small size and simple component API are attractive, but OpenAIDE would retain more custom scroll coordination around prepend, append, streaming growth, and task restoration. The library's own comparison of TanStack's reverse-scroll support is stale relative to TanStack's current chat guide and APIs.

This is a research recommendation, not approval to change production code.

## Decision

Adopt **`@tanstack/react-virtual`** for a production prototype and let it become the single owner of timeline geometry and scroll anchoring. Keep OpenAIDE as the owner of product intent: whether the user is following the live turn or reading history, when to request older messages, and when to show the jump-to-latest affordance.

The decisive current capabilities are:

- `anchorTo: 'end'` keeps the same keyed content visible across prepends and keeps an end-pinned viewport pinned while the final row changes height.
- `followOnAppend: true` follows appended rows only when already at the end.
- `measureElement` handles unknown and changing row heights with `ResizeObserver`.
- `isAtEnd`, `getDistanceFromEnd`, `scrollToEnd`, and `scrollEndThreshold` directly support OpenAIDE's following/reading UI.
- `takeSnapshot()`, `initialMeasurementsCache`, and `initialOffset` provide a better restoration seam than a raw `scrollTop` alone.
- The recommended chat arrangement uses normal DOM order rather than a CSS inversion.

These behaviors are shown together in TanStack's current [Chat guide](https://tanstack.com/virtual/latest/docs/chat) and documented in the [Virtualizer API](https://tanstack.com/virtual/latest/docs/api/virtualizer). React 19 is explicitly included in the adapter's [peer dependencies](https://github.com/TanStack/virtual/blob/main/packages/react-virtual/package.json). The July 28, 2026 release also includes a targeted fix for a one-frame viewport jump when a row above the viewport resizes, landing transform and scroll compensation in the same paint. [React adapter 3.14.9 release](https://github.com/TanStack/virtual/releases/tag/%40tanstack/react-virtual%403.14.9), [virtual-core 3.17.7 release](https://github.com/TanStack/virtual/releases/tag/%40tanstack/virtual-core%403.17.7)

## Comparison

| Option | Dynamic rows and streaming | Prepend and bottom-follow | Restore | React 19 / size | OpenAIDE fit |
| --- | --- | --- | --- | --- | --- |
| **TanStack Virtual** | Measures changing rows; explicitly retains a growing last row when pinned | Built-in stable-key prepend anchor and pinned-only append follow | Snapshot of measured rows plus offset | Explicit React 19 peer; project describes roughly 10–15 kB | **Best. Its public API matches Chat's actual contract and is headless enough to preserve the current DOM and motion.** |
| **Virtua** | Automatic variable sizing; frequently resized items may require extra work | `shift` preserves position for start mutations, but docs say to disable it for middle/end updates; pinned append remains application policy | Cache restoration may fail if item count changed | React `>=16.14`; roughly 3 kB per component | Second. Smallest and simplest initial render, but leaves more coupled scroll state in OpenAIDE. |
| **React Virtuoso** | General `Virtuoso` measures variable rows; chat-specific streaming modifiers are in Message List | General component has `firstItemIndex` and count-based `followOutput`; Message List has explicit prepend, item-change, and auto-scroll modifiers | General component supports state snapshot/restore with matching data | Explicit React 19 peer | Capable, but the exact turnkey chat API is a commercial package with license-key and EULA review overhead. The MIT general component needs manual existing-row-growth policy. |
| **react-window** | Dynamic-height cache exists, but its own README says it is less efficient than predetermined sizes | No first-class chat prepend anchor, pinned-follow, or growing-final-row contract | No timeline measurement snapshot API | React 18/19 peer | Weakest. It would preserve most of OpenAIDE's existing geometry code. |

Sources: [TanStack repository](https://github.com/TanStack/virtual), [Virtua repository](https://github.com/inokawa/virtua), [Virtua `VListProps`](https://github.com/inokawa/virtua/blob/main/docs/react/interfaces/VListProps.md), [Virtua package manifest](https://github.com/inokawa/virtua/blob/main/package.json), [React Virtuoso API](https://virtuoso.dev/react-virtuoso/api-reference/virtuoso/), [React Virtuoso package manifest](https://github.com/petyosi/react-virtuoso/blob/main/packages/react-virtuoso/package.json), [react-window repository](https://github.com/bvaughn/react-window), [react-window package manifest](https://github.com/bvaughn/react-window/blob/main/package.json).

### Why Virtua is not the final choice

Virtua is a credible option, not a bad one. It provides automatic dynamic sizing, reverse scrolling, stable-key size caching, and restoration in a very small package. The mismatch is coordination:

- Its [`shift`](https://github.com/inokawa/virtua/blob/main/docs/react/interfaces/VListProps.md) flag is specifically for insertion or removal at the start, and the docs warn that leaving it enabled for middle or end changes can produce unexpected behavior. OpenAIDE can prepend history, append messages and notices, stream into the last row, and change status rows concurrently.
- Its saved cache requires the item length to match or restoration can fail. A task can receive new rows while it is not active.
- It has no documented equivalent to TanStack's combined `anchorTo` and `followOnAppend` contract, so OpenAIDE would continue coordinating pinned-follow behavior with imperative scrolling.
- A recent reverse-chat report required special handling for simultaneous upward reading and dynamic appends. Fixes shipped, so this is not evidence the current release is broken, but it demonstrates the sensitivity of this exact interaction. [Virtua discussion 865](https://github.com/inokawa/virtua/discussions/865)

Bundle size matters, but avoiding a second geometry owner matters more for this timeline. The current hook already contains substantial resize, prepend, restoration, and intent reconciliation; selecting Virtua mainly to save several kilobytes would keep the riskier complexity.

### Why not React Virtuoso Message List

The specialized [`@virtuoso.dev/message-list`](https://virtuoso.dev/message-list/) API is the strongest turnkey component evaluated. Its [scroll modifiers](https://virtuoso.dev/message-list/scroll-modifier/) explicitly cover prepends, streaming item changes, and conditional bottom scrolling.

It is also a [commercial package](https://virtuoso.dev/message-list/licensing/) with per-developer licensing, a production license key, and EULA obligations. That creates distribution and contributor friction for an AGPL-licensed application. This report does not make a legal compatibility determination, but that review and ongoing key management are unnecessary when TanStack now provides the required primitives under MIT.

The MIT general `react-virtuoso` component remains an option, but its `followOutput` is tied to count changes; changes to the content of an existing streaming row require application coordination. The commercial Message List supplies an explicit `items-change` modifier for that distinction.

### Why not react-window

Current react-window supports a dynamic row-height cache, overscan, and imperative scrolling. Its public surface does not provide chat-specific anchoring, conditional bottom follow, prepend compensation, or measurement snapshots. Reconstructing those contracts around it would repeat the hard part already present in OpenAIDE. [react-window README](https://github.com/bvaughn/react-window)

## OpenAIDE integration shape

The current implementation makes virtualization a structural improvement rather than a component swap:

- [`TaskView.tsx`](../../packages/frontend/src/components/TaskView.tsx) renders every timeline item into one native scrolling `.message-list`.
- [`useTaskChatScroll.ts`](../../packages/frontend/src/components/useTaskChatScroll.ts) currently owns following/reading state, direct `scrollTop` writes, prepend compensation using `scrollHeight`, per-row resize and mutation observation, task restoration, and jump animation.
- [`store.ts`](../../packages/frontend/src/state/store.ts) persists only scroll ownership and raw `scrollTop` per task.
- [`ChatActivityView.tsx`](../../packages/frontend/src/components/ChatActivityView.tsx) contains disclosure state within row subtrees, so an unmounted virtual row may lose local open state unless it is keyed and stored above that row.

A production prototype should use this ownership split:

1. Build one stable timeline row model for messages and other scrollable timeline content. Use `message_id`, or an equally durable domain ID, for every virtual key. Never use array indexes.
2. Configure `anchorTo: 'end'`, `followOnAppend: true`, `measureElement`, and a modest overscan. Keep normal chronological DOM order.
3. Remove manual row measurement, `scrollHeight` prepend arithmetic, and competing geometry corrections from `useTaskChatScroll`. Retain user-intent detection, load-earlier requests, and product controls.
4. Persist a per-task virtualizer snapshot plus logical position. Treat snapshots as measurement hints rather than durable truth because only measured rows are included and unseen rows still use estimates.
5. Keep disclosure open state outside the virtual row when it must survive scrolling away and back.

TanStack's React adapter documents a `useFlushSync` option. Its current React integration guidance notes that the default synchronous flush can warn or affect performance under React 19; prototype `useFlushSync: false` and verify streaming paint behavior instead of copying a default blindly. [React Virtual guide](https://tanstack.com/virtual/latest/docs/framework/react/react-virtual)

## Animated disclosure bodies

Keep the tool-group expansion animation.

Virtualization removes the dominant global cost by unmounting settled history outside the viewport. A visible expanding group still changes the height of one mounted row, and `measureElement` observes that change while TanStack maintains the anchor. This preserves the interaction the user likes without laying out hundreds of unrelated historical rows.

Do not mix `resizeItem` and `measureElement` on the same row; TanStack documents that combination as undefined behavior. Start with the existing disclosure transition plus measured row height, then profile the real animation. Avoid enabling `useAnimationFrameWithResizeObserver` by default because the API docs say it usually adds about one frame of delay.

## Accessibility, find, and selection

All four virtualizers remove offscreen rows from the DOM. Therefore, as an architectural consequence:

- browser find-in-page cannot discover unmounted history;
- a screen reader's virtual cursor cannot traverse the complete transcript at once;
- text selection cannot extend continuously across unmounted messages.

No evaluated library eliminates this tradeoff. Mitigations should be part of the feature, not deferred as library bugs:

- provide Chat-level search over the full data model and scroll the matching keyed row into view;
- preserve semantic chronological markup and useful list or log relationships for mounted rows;
- keep a focused row mounted through a custom TanStack `rangeExtractor` if focus can otherwise disappear;
- define copy/export behavior for whole-transcript use cases;
- avoid inverted transforms, which would make source order and focus behavior harder to reason about.

A Chromium VS Code webview is not a special blocker. These libraries use normal DOM scrolling and `ResizeObserver`, and OpenAIDE already relies on those APIs. This is an environment compatibility inference; the production prototype still needs to run inside the extension host's actual webview, not only in a standalone browser. [VS Code webview documentation](https://code.visualstudio.com/api/extension-guides/webview)

## Prototype acceptance criteria

Use the real large-history fixture, including the reported 577-row case, and compare before/after traces in the VS Code extension. The prototype passes only if it verifies:

- mounted message DOM remains bounded while the scroll range represents the entire history;
- scrolling upward remains stable during streaming and incoming appends;
- the last row grows smoothly when pinned, but does not pull the user down while reading history;
- first and repeated history prepends preserve the same visible content;
- animated disclosure above, within, and below the viewport does not create visible jumps;
- switching tasks restores approximately the same logical content even when row count changed while away;
- keyboard focus, selection, jump-to-latest, and application search remain usable;
- frame time, layout cost, and long tasks improve in the extension webview, not just in a synthetic page.

The first implementation should be a bounded prototype behind the current Chat surface. Do not run both the existing scroll-compensation machinery and TanStack anchoring as competing writers; that would make any performance or correctness result inconclusive.
