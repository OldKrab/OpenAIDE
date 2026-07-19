# OpenAIDE Refactor Plan

This is the living top-level plan for the hard refactor toward two top-level modules: Backend and Frontend.

The plan is intentionally top-level. Module internals are grilled after this plan is accepted, then recorded in focused docs or ADRs before implementation. OpenAIDE is not preserving legacy structure for compatibility during this refactor; old code can be renamed, moved, or deleted when a replacement boundary is accepted.

## Goals

- Treat Backend and Frontend as the only top-level modules.
- Make App Server the source of truth for product state, workflow decisions, task lifecycle, runtime integrations, capability decisions, and persistence.
- Keep Frontend responsible for rendering product state, capturing user intent, and holding only ephemeral presentation state.
- Make best UI/UX and responsive interaction the primary product constraint across Backend, Frontend, App Shell, protocol, and runtime decisions.
- Support Web App, Desktop App, and VS Code Extension as App Shells over the same App Server Protocol.
- Reuse shared Frontend code across shells by default, with shell-specific behavior injected through narrow composition points.
- Keep the Web App a local single-user App Shell for development and real local use, not a hosted multiuser service.
- Put Rust App Server crates under `openaide-rs/`.
- Put shared TypeScript Frontend and shell packages under names that reflect product architecture rather than the current VS Code webview implementation.
- Use a Codex app-server-like split where it fits OpenAIDE, without copying Codex product nouns.

## Non-Goals

- No hosted multiuser web architecture in this refactor.
- No backwards compatibility with the current directory layout.
- No module-internal interface design in this top-level plan.
- No shell-specific product protocols.
- No VS Code API dependency in Rust App Server crates.
- No hidden local machine URLs, temporary domains, private paths, or conversation-specific setup in source, docs, package metadata, comments, or UI text.

## Focused Architecture Flows

- [Task Lifecycle and Chat specification](task-chat-flow.md): accepted client-private New Task lifecycle, Send, Native Session work, Chat updates, permissions, history synchronization, and connection recovery.

## Top-Level Module Interface

OpenAIDE has two top-level modules:

```text
Backend <-> Frontend
```

The seam between them is a typed bidirectional product protocol:

- Frontend sends user intents to Backend.
- Backend sends product snapshots, events, errors, and user-facing requests to Frontend.
- Backend may request Frontend or App Shell capabilities through the same seam.
- Frontend responsiveness follows a ladder. Local UI state is instant and needs no Backend round trip. Pending presentation gives immediate feedback while Backend decides. Optimistic reversible state may apply locally before Backend acknowledgment only when rollback is simple and honest. Backend-acknowledged state is required before showing irreversible or identity-changing product changes as real. Long-running work uses Backend streaming state.
- Slow local work such as Agent launch, ACP `session/new`, config option discovery, command discovery, authentication checks, storage recovery, and attachment validation must be represented as explicit preparing, progress, readiness, or recoverable error state. App Server Protocol methods must not require Frontend to freeze a screen while waiting for those operations to finish.
- Mutating user actions go through a central Frontend intent layer rather than direct protocol calls from arbitrary UI modules. Intent helpers classify each action in the responsiveness ladder, assign stable client request ids when needed, own pending or optimistic presentation, and reconcile Backend acknowledgments, rejections, snapshots, and events.
- Direct App Server Protocol access in Frontend is limited to bootstrap and connection modules, subscription and state-ingestion modules, Backend-initiated request handlers, and the central intent layer. Rendering modules consume derived state and call intent helpers rather than invoking protocol methods directly.
- The seam is message-oriented at the transport level but strongly typed at the binding level. Generated TypeScript bindings must typecheck method names, request params, response results, Backend events, and Backend-initiated Frontend or App Shell requests. Frontend app code must not use untyped method strings or `unknown` protocol payloads.
- The external request interface is one generic typed shape, `request(method, params, meta)`, plus typed response, event, error, and Backend-initiated request envelopes. Generated per-method helpers such as `taskCreate` or `taskSend` may exist only inside Frontend convenience layers; they are not the Backend/Frontend seam.
- Frontend may render optimistic UI for submitted user intents, but only as pending presentation state keyed by stable client request ids. Backend acknowledgments, rejections, snapshots, and events remain authoritative and reconcile, replace, or roll back optimistic state.

At category level, the Backend/Frontend contract is:

```text
BackendConnection
  initialize(...)
  request(method, params, meta)
  events()
  respond(requestId, result)
  close()

Frontend -> Backend methods
  client/*       connection and client lifecycle
  task/*         Task lifecycle, Chat, turns, and user intents
  project/*      Project Context listing, selection, and canonicalization flows
  agent/*        Agent status, setup, and settings-visible Agent data
  settings/*     Settings product state
  diagnostics/*  support export and local diagnostics flows

Backend -> Frontend events
  app/*          app-level snapshots and lifecycle
  task/*         Task, Chat, turn, and request state changes
  agent/*        Agent status and availability changes
  settings/*     settings changes
  transport/*    connection and server lifecycle signals

Backend -> Frontend requests
  shell/*        App Shell native capabilities
  permission/*   user permission decisions
  secret/*       shell-owned secret lookup or storage operations
```

Exact method records, params, result types, event payloads, and error variants are designed later. They must preserve these API ownership categories rather than creating feature-specific side channels.

Connection lifecycle guarantees:

- App Shell attaches to or launches a compatible Backend for the selected state root.
- Frontend opens the transport and must call `client/initialize` before any product request.
- `client/initialize` returns enough renderable state and server or client capability data for the client's requested initial surface without waiting for a subscription event.
- Events delivered after successful initialize are ordered relative to the returned snapshot, so Frontend can apply them without a race.
- If transport drops, Frontend reconnects with the same `clientInstanceId` when possible; Backend decides whether the reconnect resumes, requires a resync, or requires a fresh initialize.
- `close()` detaches the current Frontend client. It does not by itself mean Backend stops; Backend lifecycle follows its accepted client and state-root rules.

State synchronization guarantees:

- `client/initialize` returns a scoped renderable client snapshot plus a snapshot cursor.
- Backend events carry an ordered `previousCursor`, `cursor`, typed payload, and the state-root or client scope needed to validate application.
- Frontend applies an event only when its `previousCursor` matches the current cursor, its `cursor` advances state, and its scope matches the initialized state root and client scope.
- If Frontend detects a cursor gap, stale cursor, reconnect ambiguity, invalid event order, or scope mismatch, it stops applying incremental events and requests a fresh snapshot or resubscribe flow.
- Backend remains authoritative: events patch or replace render state, and Frontend optimistic state is reconciled by stable client request ids and durable ids.

Backend owns product state, workflow decisions, persistence, App Server process behavior, protocol source of truth, transport implementation, Agent runtime integration, and storage. App Server, storage, Agent runtime, transport, and reusable Backend clients are Backend internals with their own internal interfaces to grill later.

Frontend owns shared UI, App Shells, shell chrome, routing, menus, embedding, native capability adapters, the central intent layer, optimistic pending presentation, and other ephemeral presentation state. Web App, Desktop App, VS Code Extension, and shared rendering packages are Frontend internals with their own internal interfaces to grill later.

Frontend and App Shells must not talk directly to Backend storage or Agent runtime internals. Backend internals must not depend on VS Code or shell-specific UI APIs.

## Target Top-Level Repository Shape

```text
openaide-rs/
  app-server-protocol/
  app-server/
  app-server-transport/
  app-server-client/
  agent-runtime/
  storage/

packages/
  app-server-client/
  app-shell-contracts/
  frontend/

apps/
  web/
  desktop/
  vscode-extension/
```

The exact Rust crate split and package names remain subject to module-interface grilling. The top-level intent is fixed:

- `openaide-rs/` contains Backend implementation: App Server, protocol source of truth, transport, runtime integration, storage, and reusable Rust client boundaries.
- `packages/app-server-client/` exposes generated or thin TypeScript bindings for the App Server Protocol.
- `packages/app-shell-contracts/` is a transitional Frontend/App Shell package for the current shell-webview bridge and presentation contracts that are not App Server Protocol records. It must not define Backend product protocol semantics, and it should shrink or disappear as App Shells move to direct App Server Protocol integration.
- `packages/frontend/` contains shared Frontend product UI, rendering modules, presentation helpers, and shell injection points.
- `apps/web/`, `apps/desktop/`, and `apps/vscode-extension/` are Frontend App Shells. They own launch, connection, embedding, shell chrome, commands, menus, windows, routing, and shell capabilities.
- Frontend internals depend on shared Frontend modules and App Server client bindings. Shared Frontend must not depend on a specific App Shell.

## Current Code Mapping

- `openaide-rs/app-server/` is the current Backend seed. It still carries the old Rust crate name until Backend crate interfaces are accepted.
- `packages/app-server-client/` is the current TypeScript App Server Protocol/client bindings seed.
- `packages/app-shell-contracts/` currently contains legacy shell/webview bridge contracts and Agent catalog helpers used by the shared Frontend and VS Code App Shell. It is not a protocol source of truth.
- `packages/frontend/` is the current shared Frontend seed.
- `apps/vscode-extension/` is the current VS Code App Shell seed.
- Web and Desktop shells do not need to exist before their module plans are accepted, but the final layout reserves their places.

## Current Top-Level Architecture Status

This status supersedes counting historical workflow packets as remaining work.
The initial A0-A9 architecture slices are implemented; the active work is now
targeted cleanup, verification, and any concrete gaps found by auditing the
current code against the accepted architecture rules. Each follow-up still uses
the workflow review loop and may be implemented through small commits when
needed.

### Active Chat-State Remediation

Recent persisted-session evidence falsified several previously reported fixes:
durable sends and completed turns could remain stale in one live client; active-turn
steering could lose later Chat rows; Configuration Option changes could revive a
history-sync announcement; and Composer settlement guessed acceptance from message
text or snapshot revision. The active remediation therefore uses these stricter
contracts:

- Generic ACP prompt turns are sequential. OpenAIDE does not issue a second
  `session/prompt` while the first is pending; active Tasks expose blocked Send
  capability while keeping the local draft editable.
- One task-scoped Composer allows one in-flight `task/send`. Success clears its
  submitted draft directly; failure restores the live draft. Frontend never
  automatically retries or reconstructs a lost request.
- Every event delivered to a client shares one cursor lineage. Any cursor gap,
  including one first observed on an out-of-scope or replacement event, suspends
  incremental application until resubscription installs a baseline.
- Each App Server gateway exposes one stable, process-unique `ServerId`.
  Frontend stamps asynchronous outcomes with the matching replica epoch: a
  same-process stream resubscribe preserves history clocks and paging, a new
  process accepts its lower process-local clock, and a changed `StateRootId`
  clears every root-owned Task/cache identity before collisions are accepted.
- LocalHttp reinitializes after `notInitialized` but never replays the
  originating product method into the replacement replica. The owner receives a
  replica-changed error and resynchronizes authoritative state without replaying
  the product mutation.
- Protocol Chat message identities are authoritative. Frontend paging may dedupe
  the same identity across windows, but it must not merge distinct adjacent text,
  thought, or activity rows from whitespace, size, or position heuristics.
- Configuration Option readiness, pending mutation identity, stale/unavailable
  state, and errors survive projection into Frontend state. A ready empty catalog
  is settled, not indefinitely loading. ACP notifications proven to precede a
  set-option response are projected before the response catalog, so an older
  queued catalog cannot regress the confirmed value.
- A stale `task/send` revision conflict targets `taskRevision` and carries the
  current authoritative Task render state. Frontend restores the draft and
  surfaces the rejection; it does not parse error text or automatically retry.
- Project identity is owned independently from surviving Tasks, and one visible
  Task panel/client owns a routed Task in each App Shell.
- Stop is an independent user request rather than Send-recovery coordination. A
  cancelled ACP prompt keeps its Native Session slot until the original prompt
  response settles. A later prompt waits for that settlement instead of racing
  the Agent's active turn.
- Native history replaces cached Chat only when its activity timestamp is
  present, comparable, and strictly newer. Frontend drops retained paging rows
  only when that synchronization generation reaches `updated`; ordinary live
  growth during `syncing` preserves the reader's window and scroll ownership.
  ACP RFC 3339 activity timestamps are normalized with UTC or numeric offsets.
- Earlier-page requests carry controller-owned generations through their result
  and error actions. Clearing or replacing a paging window cannot let an older
  response settle a newer request, and active/background snapshot ingestion
  shares the same paging and terminal-request cleanup rules.
- Durable Task revision and process-local history-sync generation are independent
  clocks. Snapshot reconciliation may accept newer durable fields while retaining
  a newer history clock, and a same-generation terminal state cannot be reopened
  by a late `task/open` response.
- Pre-send attachment handles and candidates share the typed
  `attachment/release` batch contract with ordered per-resource outcomes.
  Candidate confirmation, concurrent confirmation, and release are linearized
  so cleanup cannot resurrect or duplicate resolver resources.
- Native Session identity is `(agentId, sessionId)` at every runtime, ownership,
  prompt, update, configuration, cancellation, and close seam. A session id may
  be reused by another Agent, but one Agent/session pair cannot belong to two
  Tasks even when their workspaces differ.
- An empty prepared Task has one explicit Frontend owner until first-send
  recovery becomes durable. Context replacement, existing-Task navigation,
  Native Session adoption, and Settings navigation dispose that owner exactly
  once; ordinary surface unmount preserves the Backend-recoverable draft.
  Attachment adoption transfers to a replacement prepared Task synchronously
  rather than waiting for a render.
- Task Navigation snapshots own active/archived list membership. Background
  Task snapshots may refresh an existing row and cached details, but cannot
  insert a Task omitted by the authoritative list or place an active row into
  the Archived slice.
- Process-local send recovery and cleared-attempt tombstones take precedence
  over stale browser storage. Unrelated Task/config/history errors cannot settle
  an exact send; only its keyed acceptance or keyed rejection may unlock it.
- Pending-send recovery, in-flight de-duplication, and attachment protection are
  namespaced by `(StateRootId, clientInstanceId, taskId)`. Legacy unscoped records
  are quarantined, so a colliding Task id in another root cannot receive an old
  prompt or resolver handle.
- Process replacement clears pending server requests, option/session projections,
  tool details, and editable resolver handles while retaining durable Chat and
  exact locked-send recovery. All required global subscription baselines must be
  fresh before mutations become ready again.
- Deferred zero-client shutdown remains pending while accepted turns or Task
  requests settle and is rechecked without requiring another client-expiry
  event. Repeated checks do not emit duplicate deferred-status logs.

0. **Stabilize the active ACP session termination split.**
   - Add the missing regression tests for unsupported close no-op behavior,
     unsupported delete errors, close/delete trace and ACP error mapping, and
     close while prompt execution owns the session-close path.
   - Commit the slice only after the review finding is closed.

1. **Replace the live runtime entrypoint with the App Server Protocol edge.**
   - Make the running App Server use `protocol_edge::RpcGateway` and typed
     `openaide-app-server-protocol` request envelopes for real traffic.
   - Keep compatibility shims only as temporary migration adapters, not as a
     second product protocol.
   - Current gap: `main.rs` now defaults to App Server Protocol stdio, while
     `OPENAIDE_RUNTIME_PROTOCOL=shell-control-stdio` is reserved for the
     shell-control runtime surface. The VS Code extension host owns one
     initialized LocalHttp App Server session and brokers typed session traffic
     for all of its webview surfaces. Webviews do not receive endpoint or token
     material and do not initialize independent product clients. The broken
     webview-to-stdio App Server Protocol proxy has also been removed.
     `transport/shell_control` exists only for explicit shell-local leftovers
     such as health, shutdown, and private shell helper requests.
     Legacy `runtime.health` advertises only those shell-local methods. Dotted
     product handlers have been removed from `transport/shell_control`; Task,
     Agent, permission, settings, diagnostics, and attachment product traffic
     is covered by the typed App Server Protocol edge.

2. **Implement real renderable snapshots and committed event publication.**
   - Replace placeholder `SnapshotBuilder` projections with storage/runtime
     backed client, task navigation, task, agent, project, settings, and
     pending-request snapshots.
   - Move UI update publication from legacy protocol-shaped fanout to typed
     state-sync publication after durable acceptance.
   - Frontend must be able to initialize and subscribe without polling for
     normal Task state.
   - Current progress: the App Server Protocol edge now opens storage and
     returns fallible storage-backed Task Navigation and Project Collection
     snapshots from `client/initialize` and `state/subscribe`; committed Task
     Navigation, Project Collection, and Agent Collection updates can be routed
     through generic `StateStream::publish_committed` with typed payloads.
     Project Collection updates are delivered as safe cursor-advance metadata
     for all subscription scopes so shared cursor order does not create false
     Frontend resyncs. Task subscriptions now read storage-backed Task
     snapshots through the same `TaskSnapshotStore` projection used by
     `task/open`, and protocol-edge subscription handling overlays pending
     request snapshots before returning the response. Task mutation and
     Task Product API update signaling now uses typed `task_events` updates
     instead of protocol-shaped notification messages; stdio and legacy
     adapters convert those updates at their protocol edge. The stale non-task
     ACP config-option notification fanout has been removed; ACP options
     sessions update their in-memory catalog and App Server Protocol-visible
     Task config changes are exposed through authoritative snapshots. Non-Agent
     Settings projection gaps now have typed App Server reads, runtime routing,
     source-owned empty projections, and Frontend rendering. App Server
     startup snapshots now include Backend-owned runtime/developer settings, and
     Frontend initialization stores those settings independently from legacy
     shell settings snapshots. The General Settings UI reads ACP trace state
     from the Backend runtime settings projection; the older full Settings
     snapshot fallback has been removed.
     ACP trace mutation now goes through typed `settings/updateRuntime` from
     Frontend to App Server instead of a shell-host webview message.
     Chat paging and tool detail reads now also go directly through typed
     `task/chatPage` and `task/toolDetail` Frontend BackendConnection calls;
     the VS Code webview relay and shell contract messages for those product
     reads have been removed. The remaining webview shell-message contract has
     been audited: telemetry, backend-initiated shell request forwarding,
     diagnostics, workspace roots, developer-settings unlock, surface
     navigation, local file picking, and path opening are shell/bootstrap
     capabilities. Composer submit shortcut preferences now live in typed
     App Server Settings/App Preferences protocol; Frontend uses optimistic
     local presentation and reconciles against the App Server result instead of
     posting shell-host preference messages. The remaining shell contract is
     now limited to shell/bootstrap capabilities. The transitional
     `context.pickFile`/`context.file.result` attachment bridge has been
     removed; composer file attachment goes through the App Server-backed file
     browser and App Server-owned attachment handles.

3. **Implement the target `task/*` product API and split create from send.**
   - Wire `task/create`, `task/send`, `task/open`, `task/list`,
     `task/setConfigOption`, `task/cancel`, and `task/discard` through thin
     protocol handlers into Task workflows.
   - `task/create` creates a durable Task for an existing Project and Agent,
     starts Native Session preparation asynchronously, and never sends the
     first prompt.
   - `task/send` owns prompt submission, idempotency, stale-send guards,
     authoritative acceptance, and committed Chat/turn state.
   - Current progress: `task/list`, `task/open`, `task/create`, and the first
     `task/send` acceptance path are wired through the App Server Protocol edge.
     `task/create` resolves Project and Agent identity, persists a durable idle
     Task without sending a prompt, and publishes committed Task Navigation
     updates. `task/send` accepts an idle Task prompt, persists explicit
     idempotency receipts, commits durable user-message and running-turn state,
     returns a renderable Task snapshot, and publishes delivered state-sync
     events. The first `task/cancel` path clears persisted active-turn state,
     appends canceled interruption state, and publishes delivered state-sync
     events. The first `task/setConfigOption` path persists idle Task config
     options through the shared mutation boundary and rejects running Tasks
     until live Agent option application is wired. The first `task/discard`
     path tombstones empty pre-send Tasks, returns refreshed Task Navigation,
     rejects running or non-empty Tasks, and keeps tombstoned Tasks unavailable
     through product APIs. The first Agent execution path for `task/send`
     opens or resumes a Native Session, persists the bound session id with the
     accepted turn, and spawns Agent prompt execution after durable acceptance.
     The first responsive preparation path returns a durable preparing Task
     immediately, starts Native Session preparation after durable creation,
     publishes readiness updates, allows first Send to become authoritative
     while preparation continues, and recovers abandoned preparation after
     restart as a failed preparation state.
     Agent config option metadata from session setup and live config updates is
     preserved and projected into Task snapshots. ACP slash-command updates from
     prepared, active, and loaded sessions are preserved as Task command
     readiness metadata and projected into Task snapshots. Repeated
     `task/create` for the same Project Context and Agent reopens the same empty
     Draft Task. Leaving or unmounting New Task does not discard it. First Send
     returns from durable user-message and Turn acceptance without waiting for
     Native Session acquisition or prompt execution; the background Turn runner
     waits for the existing preparation and reuses its session. Frontend keeps
     submitted text and attachments in the disabled composer until Backend
     acceptance and does not render pending Shell input as Chat.

4. **Wire `server_requests` into permissions and shell capabilities.**
   - Replace direct `HostBridge` waiters and prompt-local permission handling
     with broker-owned Backend-initiated request lifecycle.
   - Cover Task-scoped fanout, first-valid-response-wins, redelivery on
     reconnect or subscription, pending request snapshots, interruption, and
     stale response errors.

5. **Move Frontend to a typed `BackendConnection` and central intent layer.**
   - Shared Frontend must consume `@openaide/app-server-client` bindings,
     initialize through App Server Protocol, subscribe through state sync, and
     send mutations through intent helpers.
   - Rendering components must stop posting shell-control host messages such as
     `task.create` and `session.prompt`.
   - `app-shell-contracts` must shrink back to shell presentation/capability
     contracts and stop defining product protocol semantics.

6. **Move App Shell-owned product decisions into App Server.**
   - VS Code may provide native capabilities, workspace facts, secret storage,
     and embedding, but App Server must own Agent settings, Agent status,
     Project canonicalization, task lifecycle, persistence, and reload truth.
   - Extension code must stop being the source of truth for custom Agent
     definitions and Agent probing/status mapping. Runtime restart, protocol
     mode, endpoint reuse, and process sharing policy belong to A7 shared
     attach-or-launch rather than this product-decision slice.
   - Current progress: App Server Protocol `client/initialize` and Agent
     subscriptions project Backend-owned Agent collections from `AgentRegistry`.
     Typed `agent/probe` now validates Agent identity, probes through the App
     Server Agent runtime, updates App Server-owned status/cache state, returns
     a renderable Agent collection, and publishes `AgentCollectionUpdated`
     events. Project subscriptions now expose App Server-owned Project
     collection snapshots derived from visible Task history, and Task mutation
     paths publish `ProjectCollectionUpdated` cursor-safe events. Custom Agent
     mutation/settings storage and deeper Project canonicalization APIs still
     remain.

7. **Finish shared App Server attach-or-launch and reusable local transport.**
   - Implement real shell attach-or-launch using state-root fingerprints,
     endpoint records, local auth token validation, launch locking, stale
     cleanup, endpoint probe/initialize, and structured outcomes.
   - Add a browser-safe reusable local transport so Web/Desktop/VS Code can
     attach to the same App Server for a state root.

8. **Add App Server-owned Projects, Settings, Agent identity, and core product modules.**
   - Establish the remaining product state owners currently missing from the
     Rust tree: Projects, Settings, Agent definitions/identity/status, history
     policy, and recovery policy.
   - Create `openaide-rs/core` or equivalent only when the API is grilled and
     the boundary is deeper than an App Server internal module.
   - Status: completed. Projects, Settings, Agent identity, chat history
     policy, and recovery policy now have App Server-owned product modules or
     snapshot owners.

9. **Add attachment runtime and file browser lifecycle.**
   - Implement App Server-owned pre-send handles, embedded candidates,
     validation refresh, release, live open/reveal routing, allowed-root file
     browsing, TTL cleanup, and send-time consumption.
   - Keep raw paths and file contents out of Frontend state and protocol
     snapshots according to the accepted attachment rules.
   - Status: completed. Backend pre-send attachment runtime, `task/send`
     handle validation, typed file browser roots/listing, public file-reference
     handle creation, refresh/release, embedded candidate confirmation, TTL
     cleanup, send-time handle consumption, Frontend handle-backed send/remove
     integration, and task composer file browser UI for reference/embed
     selection are implemented. Live open/reveal routing is implemented through
     `attachment/reveal` and same-client `shell/revealFile` delivery.

After those slices, the cleanup phase deletes superseded legacy protocols,
bridge contracts, polling paths, and runtime shims, then adds end-to-end Web,
Desktop, and VS Code smoke coverage.

## Refactor Phases

1. **Top-Level Plan**
   - Record this plan.
   - Grill and accept the target repository shape, package ownership, and phase order.
   - Update root and folder `AGENTS.md` rules when the ownership model changes.

2. **Repository Shape**
   - Rename and move top-level directories.
   - Update workspaces, package names, build scripts, and per-folder rules.
   - Keep changes mechanical where possible.

3. **App Server Protocol Boundary**
   - Make protocol records and method/event shapes App Server-owned.
   - Generate or expose TypeScript bindings through the thin client package.
   - Keep transport-neutral semantics separate from transport mechanics.

   Accepted boundary decisions:
   - Extract `openaide-rs/app-server-protocol` first as the next real Backend boundary.
   - `app-server-protocol` is a pure contract crate: serializable protocol types, method/event names, envelopes, stable error codes, and tiny deterministic structural helpers only.
   - `app-server-protocol` must not contain storage, Agent runtime, App Server lifecycle, transport I/O, shell capability execution, or UI/rendering helpers.
   - Rust protocol types in `app-server-protocol` are the source of truth for TypeScript bindings.
   - `packages/app-server-client` consumes generated or mechanically derived TypeScript protocol bindings and may add thin runtime helpers, but must not redefine protocol semantics.
   - The `app-server-protocol` TypeScript generator is a binding assembly boundary, not a protocol semantics module. Its facade owns `bindings()` and output order; focused private generator modules own method constants, Rust type declarations, and typed method maps or aliases. Adding protocol records must update the right generator group and then run `npm run protocol:generate` plus `npm run protocol:check`.
   - Transport-neutral request, response, event, and error envelope shapes belong in `app-server-protocol` when they describe product protocol semantics.
   - Concrete wire/framing concerns such as JSON-RPC parsing, stdio, websocket, HTTP, reconnect, endpoint discovery, and local auth tokens stay outside `app-server-protocol`.
   - The App Server Protocol API is designed greenfield from the accepted product architecture. The current Rust `protocol/` module is migration input, not the API shape to preserve or mechanically extract.
   - The App Server Protocol is a product-intent API: Frontend sends user intents and lifecycle intents, Backend returns render-ready snapshots, events, errors, and requests. Protocol methods must not mirror storage CRUD, ACP calls, runtime implementation objects, or current internal module shapes.
   - The protocol is snapshot-first for recovery and baseline rendering: `client/initialize` returns complete renderable state for the initialized client scope plus a cursor, and mutations return updated relevant state or accepted operation state.
   - Live Agent output still streams as ordered events, including chunk events. Backend normalizes chunks into durable Chat and Task state, and reconnect or reload uses fresh snapshots rather than raw chunk replay as the source of truth.
   - The first protocol surface to design is `client/initialize -> InitializeResult`, whose main payload is `ClientSnapshot`.
   - `ClientSnapshot` is the complete renderable baseline for this initialized client scope, not a global app dump or database dump. It is shaped by initialize params such as shell kind, requested initial surface, Project Context, Task id, and client capabilities.
   - `ClientSnapshot` includes only the categories needed to draw that client surface without guessing, such as relevant server/client identity, state-root scope, capabilities, Project context, Agents and Agent Status, Task navigation summaries, an active Task snapshot, Settings summary, pending requests, and the event cursor when those categories are relevant to the requested surface.
   - `task/create` creates a real durable Task immediately; Frontend must not invent draft Tasks.
   - `task/create` requires an existing `projectId` and `agentId`. The Backend resolves the Project to the ACP `cwd` and allowed root scope, then starts native Agent session preparation.
   - `task/create` must return after durable Task creation and accepted preparation, not after all Agent setup completes. ACP `session/new`, config option discovery, command discovery, authentication checks, and related setup readiness arrive through Task snapshot state and ordered events.
   - A Task's Project and Agent identity are immutable after creation. Changing either on a new-task surface creates a different Task/session boundary and cleans up the old empty Task/session when possible.
   - `task/send` always targets an existing `taskId` and never creates a Task implicitly. The first prompt is sent only when the user submits it.
   - ACP slash commands are advertised live session state, not separate Backend methods. Task snapshots expose available slash commands with explicit loading/stale/unavailable/failed state; selecting a slash command composes normal user prompt text and final execution still uses `task/send`.
   - `task/create` returns the accepted render state as `TaskSnapshot`; setup/preparation state lives inside the snapshot so initialize, events, reconnect, and method results use one render model.
   - `TaskSnapshot` exposes explicit preparation, Agent config, slash-command, and send-capability state. Slow or unavailable state must be visible as loading, preparing, stale, unavailable, failed, blocked, or ready state instead of inferred by Frontend.
   - ACP session config options are live Agent/session state, not durable authoritative OpenAIDE Task state. `task/create` does not accept config option values; Backend refreshes options from ACP session setup, load, resume, `session/set_config_option` responses, and `config_option_update` notifications.
   - Config option changes use one generic `task/setConfigOption` method keyed by Task, config id, value, and client mutation id. Frontend may show option changes optimistically as pending presentation, but Backend-returned Task snapshots and events are authoritative. OpenAIDE allows config option changes while a turn is running because ACP allows it, but UI must attribute pending, confirmed, and failed changes clearly.
   - `task/send` v1 sends one composer message: optional normalized text plus ordered message-level attachment handles. Slash commands are normal text in that body. Future protocol/storage shapes may support inline parts, but v1 UI and send API use one text body plus ordered attachments.
   - `task/send` uses one in-memory pending presentation. Backend success returns the committed `turnId`, committed `userMessageId`, and updated `TaskSnapshot`; any request failure restores the editable live composer with errors; Frontend never automatically retries a lost response.
   - `task/send` includes a stale-send guard such as a Task or composer revision. Backend validates readiness, config, attachments, allowed roots, capabilities, and message content authoritatively once; stale or conflicting sends return structured errors plus updated render state.
   - The first `task/*` protocol slice includes `task/create`, `task/send`, `task/setConfigOption`, `task/cancel`, `task/open`, `task/list`, and `task/discard` for empty or pre-send Tasks.
   - `task/open` loads or focuses an existing Task without reinitializing the client. `task/list` supports Web/Desktop all-project history and VS Code project-scoped history through filters. `task/cancel` is core Task lifecycle. `task/discard` cleans up empty new-task sessions when the user changes Project/Agent or leaves the new-task flow.
   - Historical Task deletion is a separate feature, not part of the first `task/*` protocol slice; it must account for local storage deletion, Agent native delete capability when present, and user-visible warnings.
   - `state/subscribe` and `state/unsubscribe` are the first centralized subscription methods. Subscribe takes a typed product scope and returns an initial snapshot plus cursor; following App Server events must chain from that cursor through `previousCursor`.
   - Subscription scopes are product scopes, not transport routing internals: Projects, Agents, Settings, Task Navigation with optional Project filter, and Task. Event scopes still carry routing facts such as state root, initialized client, and Task id.
   - Subscription snapshots are render-ready product snapshots: Project collection, Agent collection, Settings, Task Navigation, or Task. Frontend must not reconstruct product decisions from lower-level storage or Agent records.

4. **App Server Split**
   - Split current backend runtime into accepted App Server modules.
   - Separate product lifecycle, Agent runtime, storage, transport, App Shell capability requests, and reusable client boundaries.
   - Keep Rust independent from VS Code APIs.

   Accepted App Server module groups:

   - `protocol_edge`: JSON-RPC and App Server Protocol edge for initialized transports. Main external module is `rpc_gateway`; internal modules may include `protocol_dispatcher`, `response_router`, and `error_mapper`. It owns envelope lifecycle, initialize gating, response routing for Backend-initiated requests, and protocol error mapping. It must not own product decisions.
   - `client_lifecycle`: initialized App Shell client identity above raw transports. Main external module is `client_hub`. It owns client identity, transport attach or close observations, reconnect grace, client capability facts, deterministic lifecycle outcomes, and client delivery ports. It does not own product subscriptions, pending request routing policy, or product workflow state.
   - `app_lifecycle`: AppServer process lifecycle. It owns `running`, `draining`, and `stopping` transitions, last-client shutdown decisions, graceful shutdown planning, and initialize behavior during draining or stopping. It consumes client lifecycle outcomes but does not own client identity, subscriptions, pending request policy, or Task recovery implementation.
   - `state_sync`: subscription and event continuity. Main external module is `state_stream`; internal modules may include `subscription_index`, `cursor_sequencer`, and `event_fanout`. It owns subscription indexes, subscription identity, cursors, event fanout, unsubscribe cleanup, and reconnect-aware suspension or expiry of delivery.
   - `snapshots`: read-only renderable protocol projection. Main external module is `snapshot_builder`; internal modules include `task_projection`, `task_navigation_projection`, `agent_projection`, `project_projection`, `settings_projection`, and `pending_request_projection`. Snapshot building must not mutate product state, start runtime work, call transports, or depend on `state_stream` or `client_hub` except through value inputs or read-only views.
   - `server_requests`: Backend-initiated request lifecycle and shell capability routing. Main external module is `server_request_broker`; internal modules may include `capability_broker` and responder-set tracking. It owns pending request state, task-scoped fanout, first-valid-response-wins, late-response errors, stale or interrupted requests, capability target selection, and pending request rows for snapshots.
   - `protocol_handlers`: thin protocol resource handlers such as `client_handlers`, `state_handlers`, `task_handlers`, `agent_handlers`, `project_handlers`, and `settings_handlers`. They validate params, call one deeper workflow or module interface, and map results to protocol responses. Product invariants and workflow decisions must not accumulate in handlers.
   - `task_workflows`: named Task workflow modules such as `task_creation`, `task_opening`, `turn_runner`, `task_cancellation`, and `pre_send_discard`. These modules coordinate core product modules, storage, Agent runtime, server requests, and state publication for specific Task workflows.
   - `agent_workflows`: live Agent orchestration such as `agent_runtime`, `history_sync`, and `agent_status_refresh`. ACP process/session I/O and update observation stay here or in the ACP integration crate; Agent identity and settings decisions stay in core product modules.
   - `attachment_runtime`: App Server-owned local attachment and file browser lifecycle. It owns attachment resolvers, allowed-root validation, file browser cursors, pre-send handles, embedded candidates, attachment release, TTL cleanup, and mediated open or reveal routing. These concerns must not hide inside generic Task handlers.
   - `storage_runtime`: App Server storage and process-safety mechanics around the state root. It owns state-root store access, live Native Session lease coordination, storage transactions, commit/outbox integration for event publication, endpoint/runtime records where applicable, and crash recovery coordination. Core product modules still own product storage policy and data invariants.
   - `core_product`: reusable product modules in `openaide-rs/core`, including `projects`, `agents`, `tasks`, `settings`, `chat_history`, `recovery`, and storage policy. App Server calls these modules; product invariants do not live in App Server edge handlers.

   Accepted App Server split invariants:

   - `client/initialize` is a coordinated edge flow: it registers or reattaches the initialized client, returns a renderable baseline `ClientSnapshot`, and returns a baseline cursor from the same coherent read view. It creates no product subscriptions. Product stream events start only after `state/subscribe`.
   - `state_stream.subscribe` owns the subscription critical section. It registers the subscription, chooses the cursor, and calls a `SnapshotProvider` with a `SnapshotReadToken`. If it returns cursor `N`, the next delivered event for that stream must have `previousCursor = N`.
   - `SnapshotReadToken` means the snapshot is read under a coherent cursor/read view. Only state synchronization or initialize coordination may mint it, and only snapshot projection consumes it. Callers must not need to understand storage transactions or cursor allocation to use snapshot interfaces correctly.
   - Subscription identity is based on initialized `ClientInstanceId` plus scope, not raw `ConnectionId`. Transport close suspends delivery; reconnect updates delivery; client expiry may drop subscriptions.
   - `client_hub` owns factual client state and delivery only. `state_stream` owns all subscription indexes. `server_request_broker` owns pending request lifecycle and request routing policy.
   - `server_request_broker` reacts to client lifecycle, subscription changes, and capability changes so Task-scoped prompts can be delivered, redelivered, dismissed, interrupted, or failed consistently.
   - `state_stream.publish_committed` may publish only after the corresponding product state is durably accepted. App Server must not emit Agent output or Task state as durable UI before the accepted storage commit or equivalent commit/outbox record exists.
   - `rpc_gateway` rejects non-`client/initialize` requests before initialization and rejects or routes initialize according to `app_lifecycle` state. Initialize during `draining` may abort draining; initialize during `stopping` returns a structured stopping error.
   - Lifecycle methods return closed deterministic outcome enums and accept an explicit clock value where timing matters. Tests must not rely on sleeping.
   - Backend-initiated response handling returns a closed `ResponseOutcome`, including accepted, invalid, already resolved, and unknown request outcomes. Late responses must not mutate accepted state.
   - `task/discard` remains pre-send or empty-Task cleanup only. Historical Task deletion is a separate feature.

   First App Server implementation slice:

   - Status: completed as the initial skeleton baseline.
   - Implemented skeletons for `protocol_edge`, `client_lifecycle`, `state_sync`, `snapshots`, minimal `app_lifecycle`, and minimal `storage_runtime` cursor/read-token support.
   - Covered `client/initialize`, `state/subscribe`, and `state/unsubscribe` before Task workflow implementation.
   - Tests cover: non-initialize request before initialize is rejected; initialize records a client and returns snapshot plus cursor; initialize uses the state stream cursor lineage; reconnect with the same `clientInstanceId` reattaches delivery; reinitialize replaces stale connection ownership; transport close enters reconnect grace without immediate expiry; grace expiry is deterministic; subscribe returns snapshot plus cursor and stores the subscription; first event after subscribe has `previousCursor` equal to the subscribe cursor; unsubscribe after an active subscription removes delivery; unsubscribed scopes receive no delivery; client-scoped events deliver only to the matching client; reconnect moves later event delivery to the new connection; initialize during stopping returns `serverStopping`.

   Completed App Server API slice:

   - Grill and design `server_requests` before Task workflow implementation depends on permissions, shell capabilities, or secret requests.
   - The slice should define the `server_request_broker` interface, request ownership model, response outcomes, task-scoped fanout, client-originated request failure behavior, stale/late response errors, snapshot projection inputs, and lifecycle hooks from `client_lifecycle` and `state_sync`.
   - The slice should not implement Task workflows, Agent ACP I/O, shell capability execution, or durable recovery. It should provide the in-memory request lifecycle boundary those later modules call.

   Accepted `server_requests` API contract:

   - `server_requests` is the single App Server module that owns live Backend-initiated request state for `permission/*`, `secret/*`, and `shell/*` requests. Task workflows, Agent runtime, attachment runtime, and app lifecycle call the broker rather than creating their own waiters, pending maps, or response races.
   - The main external module is `server_request_broker`. Its interface is intentionally small: open a request, handle a response, interrupt or fail requests by scope, observe client or subscription lifecycle facts, and project pending request snapshots.
   - The broker owns request ids, request method classification, pending request records, responder eligibility, delivery attempts, first-valid-response-wins resolution, late/stale response errors, interruption state, and renderable pending request snapshot rows.
   - The broker does not execute shell capabilities, decide Task workflow policy, persist recovery state, perform Agent ACP I/O, own subscriptions, own client identity, or publish durable product events directly. Callers persist product state and publish committed events after broker outcomes make that appropriate.
   - Request scope is explicit. `Client` scope targets one initialized `clientInstanceId` for owner-only shell capabilities such as opening or revealing a live composer attachment. `Task` scope targets a `taskId` and can be answered by any eligible initialized client subscribed to that Task according to broker responder policy.
   - Task-scoped requests survive individual client disconnects while the App Server process lives. If one eligible client disconnects, the request remains pending for other eligible clients and for the same client if it reconnects before expiry. If no clients are currently subscribed to the Task but at least one initialized App Shell client remains, the request stays pending and appears when a client subscribes to that Task again. Client-scoped requests fail or are interrupted when the originating client disconnects before answering.
   - Task-scoped response resolution is first-valid-response-wins. Once a valid response resolves a request, later responses return `requestAlreadyResolved` or another stable stale/resolved error and must not mutate accepted state.
   - Pending requests are live memory state, not durable actionable state. After App Server restart, affected Tasks recover as interrupted or detached Task state through Task recovery workflows; the broker does not resurrect answerable prompts from storage.
   - Broker response handling returns a closed `ResponseOutcome` enum with at least: accepted, invalid response, already resolved, unknown request, unauthorized responder, stale request, and interrupted. The exact enum may split or rename cases during implementation, but callers must not infer outcomes from strings or optional values.
   - Opening a request returns a closed `OpenRequestOutcome` with the pending snapshot row plus delivery instructions for currently eligible clients, or a structured no-eligible-responder/capability-unavailable outcome when the request cannot be presented. It must not block waiting for a user or shell response.
   - Delivery is separate from opening. The broker returns `Delivery` records or server request envelopes for `protocol_edge` to send through current client delivery ports. If a client later becomes eligible, reconnects, subscribes, or gains capability, broker lifecycle hooks may return redelivery instructions.
   - Snapshot projection is read-only. `snapshots` asks the broker for pending request rows by client scope and Task scope; the broker returns protocol-safe `PendingRequestSnapshot` data only, never raw shell payloads, secret values, local paths, resolver internals, or executable capability parameters.
   - Broker lifecycle hooks consume facts from `client_lifecycle` and `state_sync`: client initialized or reattached, transport unavailable, client expired, subscription added or removed, capability state changed, and Task interrupted or completed. These hooks return deterministic outcomes and delivery or invalidation effects; they do not reach into those modules' internals.
   - Tests for the first implementation must prove: opening is non-blocking; client-scoped request fails on originating client disconnect; Task-scoped request survives one client disconnect; Task-scoped request remains pending without current subscribers; first valid Task-scoped response wins; late response returns a stable stale/resolved error; unauthorized client cannot answer; interruption prevents later mutation; snapshot projection includes only safe rows; lifecycle redelivery happens when an eligible client appears.
   - Implementation status: first in-memory `server_requests` broker slice is implemented under `openaide-rs/app-server/src/server_requests/`. It is intentionally not wired into Task workflow, Agent ACP I/O, App Shell capability execution, durable recovery, or protocol-edge delivery yet.

   Accepted process lifecycle, shared-instance discovery, and state-root API contract:

   - The slice has three external seams: `app_lifecycle` for one running App Server process, `storage_runtime` for state-root identity and process-safety facts, and `app-server-client` for reusable shell attach-or-launch. App Shells call `app-server-client`; App Server runtime calls `app_lifecycle` and `storage_runtime`; protocol handlers and Task workflows do not perform endpoint discovery, launch locking, state-root fingerprinting, or storage writer arbitration directly.
   - `app-server-client` owns attach-or-launch mechanics for all shells: state-root fingerprint input, runtime endpoint record lookup, launch lock acquisition, stale endpoint cleanup, endpoint validation probe, compatible existing-server reuse, process launch request construction, local auth token loading from protected runtime endpoint records, and structured attach-or-launch outcomes. It must not own product Settings, task history, UI routing, shell commands, normal App Server Protocol traffic, or durable OpenAIDE product state.
   - `storage_runtime` owns state-root normalization, stable state-root fingerprinting, runtime/cache directory placement for endpoint records, endpoint record read/write/delete primitives, launch-lock primitives, storage writer protection, storage-open compatibility checks, and crash-recovery classification facts. It must not own App Shell process launching, protocol transport dispatch, product Task decisions, Agent runtime I/O, or Frontend rendering state.
   - `app_lifecycle` owns in-process lifecycle reduction after an App Server is running: `running`, `draining`, `stopping`, initialize admission, last-client shutdown decisions, reconnect grace effects, graceful shutdown planning, and shutdown completion classification. It consumes factual outcomes from `client_lifecycle`, `storage_runtime`, Task recovery, and Agent/runtime shutdown, but it does not own client identity, product subscriptions, endpoint discovery, storage locks, pending request policy, or Task recovery implementation.
   - State-root identity is explicit and shell-neutral. App Shells provide a local state-root path or resolved profile root to `app-server-client`; the client asks shared code to normalize and fingerprint it. Fingerprints are stable for the same canonical local root and are used only for runtime discovery and storage coordination, not as user identity or telemetry.
   - Runtime endpoint records are per state-root fingerprint and live in OS runtime/cache storage, never inside durable product state. Records contain only process and transport facts needed for local reuse, including server id, pid/start identity when available, endpoint addresses, supported transport kinds, protocol/app compatibility, state-root fingerprint, lifecycle hint, and a high-entropy process-scoped local auth token reference or value stored with protected permissions.
   - Endpoint records are hints. `app-server-client` must probe and initialize an endpoint before reuse; failed probe, auth failure, incompatible protocol/app version, mismatched state-root fingerprint, stale pid/start identity, or stopping lifecycle response makes the record non-authoritative and eligible for cleanup or replacement.
   - The first implementation may support one reusable browser-safe local transport plus stdio launch/parent transport if that matches current repo shape, but the interface must allow multiple attach transports. Stdio can bootstrap a launched process, but it must not be the only reusable endpoint once cross-shell reuse is expected.
   - Local transport auth is process-scoped access control. It is not user identity, not client identity, and not product authorization. Shells receive it only through protected endpoint records or launch results; browser Frontend receives only ephemeral connection info from its Web App shell/bootstrap layer.
   - App Server process lifetime is based on initialized App Shell clients tracked by `client_hub`, not raw transport connections. Reconnect grace defaults to 10 seconds. `app_lifecycle` starts draining only after the last initialized client expires or detaches; a new successful initialize during draining aborts draining; initialize during stopping returns a structured stopping error.
   - Graceful shutdown order is explicit: stop accepting new work, persist accepted Chat and Task interruption/detach state, interrupt live pending server requests, stop or detach Agent transports, flush storage/outbox effects needed for coherent recovery, release live Native Session ownership only if coherent persistence succeeded, remove endpoint records, then report stopped. If coherent persistence fails, do not perform a clean live ownership release; let leases expire and let the next recovery classify the shutdown as unclean.
   - Storage writer protection is mandatory even if several shells start concurrently. Launch locking prevents duplicate App Server startup for a state root, and storage writer guards prevent two live App Server processes from mutating the same state root. A losing launcher must either attach to the winner after probe or return a structured attach/launch failure.
   - `app-server-client` attach-or-launch returns closed outcomes: attached existing server, launched and attached server, launch in progress/retryable wait, incompatible existing server, auth or permission failure, stale endpoint cleaned and retryable, launch failed, and storage/state-root blocked. Callers should not parse logs or strings to decide UX.
   - `storage_runtime` recovery classification returns closed outcomes such as clean open, unclean previous shutdown requiring Task recovery, storage locked by live server, incompatible state schema, endpoint-record stale, and unrecoverable storage error. Product recovery workflows decide how to render or continue Tasks after receiving these facts.
   - `protocol_edge` remains a protocol seam. It may call `app_lifecycle` for initialize admission and shutdown-related errors, but it must not read endpoint records, acquire launch locks, derive state-root fingerprints, or decide process lifetime directly.
   - Tests for the first implementation must prove: same state root yields the same fingerprint; different roots do not collide in normal cases; endpoint records are not stored in durable product state; stale endpoint records are cleaned only after failed authoritative probe; attach-or-launch reuses a compatible live server; concurrent launch attempts elect one writer/launcher; initialize during draining aborts draining; initialize during stopping is rejected; last-client expiry starts draining; reconnect before expiry prevents shutdown; storage locked by another live server returns structured blocked outcome; clean shutdown removes endpoint records; unclean shutdown is classified for recovery without auto-resuming Agent work.
   - Implementation status: first narrow process lifecycle/state-root slice is implemented with `app_server_client`, expanded `app_lifecycle`, and split `storage_runtime` primitives. It intentionally does not wire real shell launchers, reusable browser transport, durable Task recovery, Native Session takeover, or broad storage migrations yet.

   Accepted storage model and concurrent access protection API contract:

   - `storage` owns durable product files and product persistence helpers. `storage_runtime` owns process-safety mechanics around opening and using a state root. Task workflows and Agent workflows must call `Store` or narrower storage modules; they must not open lock files, infer clean shutdown markers, or inspect runtime endpoint records directly.
   - `Store::open` becomes the only normal entry point for durable product storage. It resolves the state root, acquires or receives a process-level writer guard, creates required directories, validates schema/open compatibility, records an open runtime marker, and returns a `StoreOpenOutcome` or structured storage error. Existing tests may use an explicit test-only open path that still exercises the same invariants.
   - The process-level storage writer guard is held for the lifetime of the live App Server `Store`. It prevents two App Server processes from mutating the same state root. In-process mutation mutexes may remain for ordering within one process, but they are not a substitute for the cross-process writer guard.
   - `Store` must keep the writer guard private and non-clonable; cloned storage handles may share an internal guard owner but callers cannot drop, transfer, or bypass it. No product module receives a raw lock path or lock file handle.
   - Storage clean/unclean state is durable product-runtime metadata under the state root, distinct from OS runtime endpoint records. On open, `storage_runtime` classifies the previous run as clean, unclean, schema-incompatible, locked by live writer, or unrecoverable. It does not auto-resume Agent work; Task recovery uses the classification later.
   - Storage writes that affect user-visible product state must go through a commit seam that can return both the accepted mutation result and event/outbox facts. The first implementation may keep current file writes, but the interface should make it possible for `state_sync.publish_committed` to run only after durable acceptance.
   - The first storage transaction interface should be small: begin or execute a named mutation under the in-process mutation lock, perform atomic file writes through storage helpers, return a closed commit outcome, and expose enough commit metadata for future event publication. It must not expose ad hoc filesystem paths beyond existing safe storage module methods.
   - Atomic file replacement remains the default file-store primitive for JSON and JSONL rewrites. Append-only files must either use explicit append helpers with fsync policy or remain behind current storage modules until a later slice designs append durability. Callers must not mix direct `std::fs` writes with storage-managed records for product state.
   - Schema compatibility is explicit even if v1 has only one schema. `Store::open` returns a structured incompatible-schema outcome rather than partially opening a state root that cannot be safely mutated.
   - Runtime diagnostics and support export may report redacted storage state such as root fingerprint, schema version, recovery classification, and lock status, but must not expose raw endpoint auth tokens, runtime lock paths, or private local paths beyond existing safe product display rules.
   - Tests for the first implementation must prove: second `Store::open` for the same state root is blocked while the first writer guard lives; dropping the first store releases the writer guard; `Store::open` classifies an unclean previous shutdown; a clean close writes a clean marker; schema mismatch returns a structured error; storage runtime endpoint records remain outside durable product storage; storage mutation helpers still serialize in-process updates; and existing Task storage tests still pass without direct lock-file knowledge.
   - Implementation status: first narrow storage concurrency/open-safety slice is
     implemented and reviewed. It intentionally does not add the later Task mutation
     commit/outbox seam.

   Accepted Task mutation commit seam and workflow split API contract:

   - `task_mutations` is the Task-workflow module that owns durable Task mutation
     commits. Workflow modules call it instead of open-coding lock, revision, write,
     message-history refresh, and notification behavior.
   - The commit interface executes a named Task mutation under the in-process mutation
     lock, exposes only a controlled mutation context, assigns revisions exactly once,
     refreshes message history when Chat changed, persists through `Store`, and
     returns a closed `TaskCommitOutcome`.
   - `TaskCommitOutcome` is the only post-commit publication fact surface for Task
     workflows. It includes the affected Task id, committed revision, navigation
     update facts, Task snapshot needs, Chat event facts when available, and snapshot
     read instructions needed by method results.
   - Failed persistence returns an error and no `TaskCommitOutcome`; rejected or no-op
     mutations return closed non-committed outcomes without advancing revisions,
     refreshing message history, or producing publication facts.
   - `task_mutations` does not own transport routing, subscriptions, App Shell clients,
     protocol delivery, or Agent runtime effects. `state_sync` remains the ordered
     event publisher, and later integration translates commit outcomes into
     `AppServerEventPayload` only after durable commit succeeds.
   - New Task workflow code must not add direct protocol-shaped update fanout.
     Durable Task mutation paths publish typed Task update facts through the
     Task mutation seam and protocol edges translate those facts only at their
     boundary.
   - The first implementation deepens existing `tasks::mutation::TaskMutations`,
     migrates only simple Task-only write paths, and adds tests for revision assignment,
     no-op mutation behavior, message-history refresh, and publication facts.
   - Implementation status: first narrow Task mutation commit seam slice is
     implemented and reviewed. It migrates `markRead`, archive/restore/tombstone,
     Task transition helpers, active turn event writes, and config-option updates
     through `TaskMutations::commit_existing_task`; adds callback side-effect
     rollback; proves no-op and committed native-delete ordering behavior; and keeps
     `TaskTurnLifecycle` create, prompt start, and permission response paths as the
     explicitly documented next migration surface.
   - Accepted next slice: migrate the remaining `TaskTurnLifecycle` durable Task
     mutations into `TaskMutations`. `TaskTurnLifecycle` keeps Agent session
     start/load/resume/close, event-sink attachment, turn spawning, and cleanup
     policy; `TaskMutations` owns prompt follow-up commits, permission response
     commits, and new Task creation commits with initial Chat history.
   - The next slice adds a Task creation commit interface for new Task records plus
     initial normalized Chat messages. It must remove `next_revision`,
     `Store::write_task`, and direct message persistence calls from
     `TaskTurnLifecycle` for migrated paths.
   - Ordering for the next slice: Agent session start/load may happen before durable
     creation when required by Agent APIs, but spawning a turn and post-commit external
     side effects happen only after `TaskMutations` returns a committed outcome. Any
     failure after an Agent session is opened must close, invalidate, or finalize using
     the existing cleanup paths.
   - Implementation status: `TaskTurnLifecycle` migration slice is implemented and
     reviewed. Prompt follow-up, permission response, prompt-start creation, and
     adopted-session creation now commit durable Task and Chat changes through
     `TaskMutations`. Agent session start/load/resume/close, event-sink attachment,
     turn spawning, and cleanup policy remain in `TaskTurnLifecycle` or `TurnRunner`.
     Response snapshots for migrated mutation paths are produced by the mutation seam.
     Tests cover create rollback after initial Chat writes, permission waiter atomicity,
     migrated-path static bypass guards, and resumed-session attach failure behavior.
   - Implementation status: `TaskService` facade split slice is implemented and
     reviewed. `TaskQueries` owns read-only list, diagnostics, snapshots, Chat paging,
     and tool details through a narrow `TaskReadStore` wrapper. `TaskCommands` owns
     mark-read and archive/restore/delete through `TaskMutations`. `TaskTurnLifecycle`
     remains the owner of create, prompt, cancel, permission response, shutdown turn
     cleanup, and volatile recovery. Agent probe/auth/list/config operations remain in
     `TaskService` until a separate Agent service slice is accepted.
   - Proposed next slice: split Agent-facing public operations out of `TaskService`
     into an internal `AgentService` while preserving protocol method names and
     results. `AgentService` owns registry validation, Agent probe/auth/session-list,
     and prepared config option requests. `TaskService` keeps the public facade and
     delegates Agent methods, while `TaskTurnLifecycle` remains the owner of Task
     creation and turn orchestration. This slice must not move Agent session
     start/load/resume/close or Native Session lifecycle policy out of Task workflows.
   - Implementation status: Agent service split slice is implemented and reviewed.
     `AgentService` owns public Agent utility operations and request validation,
     while `TaskService` keeps stable facade delegates for the current transport
     dispatcher. Task creation, turn orchestration, Agent session start/load/resume,
     and Native Session lifecycle policy remain in Task workflow modules.
   - Proposed next slice: move the large inline ACP runtime tests out of
     `agent/acp.rs` into a separate Rust test submodule. This is a test-layout and
     production-boundary cleanup only: `AcpAgentRuntime` remains the public ACP runtime
     facade, `AcpRuntimeKernel` remains the implementation owner, and no ACP behavior,
     protocol mapping, or Agent lifecycle logic changes in this slice.
   - Implementation status: ACP test-layout split is implemented and reviewed.
     `agent/acp.rs` now contains only the small `AcpAgentRuntime` production facade
     plus an external test-module declaration, and the existing ACP runtime tests live
     in `agent/acp/tests.rs`.
   - Proposed next slice: extract ACP prepared config-option application helpers from
     `agent/acp_runtime_kernel.rs` into a focused module. The new module owns
     config-option selection parsing, set-option request dispatch, and draining prior
     session updates while applying selected options. `AcpRuntimeKernel` remains the
     runtime owner for process/session registries and active
     ACP session lifecycle.
   - Implementation status: ACP config-option application split is implemented and
     reviewed. `agent/acp_config_options_apply.rs` now owns selected option parsing,
     set-option request dispatch and Task session startup; `AcpRuntimeKernel` still owns runtime/session
     registries and lifecycle.
   - Proposed next slice: extract ACP prompt execution helpers from
     `agent/acp_session_worker.rs` into a focused prompt runner module. The new module
     owns prompt content building, prompt request dispatch, prompt update projection,
     prompt cancellation, close-while-prompt handling, and current prompt host
     capability registration. `AcpSessionWorker` remains the owner of session startup,
     command loop, session event sink/catalog delivery, delete, and worker lifetime.
   - Implementation status: ACP prompt runner split is implemented and reviewed.
     `agent/acp_prompt_runner.rs` now owns prompt request construction, prompt
     response handling, prompt cancellation, close-while-prompt handling, tracing, and
     active prompt update projection. `AcpSessionClient` no longer pre-validates prompt
     content, so prompt-turn content building and validation have one owner.
   - Proposed next slice: split ACP update projection responsibilities out of
     `agent/acp_update_projection.rs` into focused internal modules for live prompt
     projection, replay projection, and config-option/session config projection. The
     caller-facing projection APIs stay stable, and this slice must not change Agent
     event mapping, replay history mapping, permission behavior, config catalog shape,
     or ACP session lifecycle.
   - Implementation status: ACP projection split is implemented and reviewed.
     `agent/acp_update_projection.rs` is now a thin stable re-export layer, while
     live prompt projection, replay projection, config projection, and shared tool-call
     merge behavior live in focused modules under the Agent ACP internals.
   - Proposed next slice: move `agent/prompt_content.rs` inline tests into
     `agent/prompt_content/tests.rs`. The production prompt-content module keeps the
     same API and behavior; this slice exists to satisfy Rust test-layout and source
     size rules before any prompt attachment behavior changes are considered.
   - Implementation status: prompt-content test-layout split is implemented and
     reviewed. `agent/prompt_content.rs` now contains production prompt conversion and
     validation code plus an external test-module declaration, and the prompt-content
     unit tests live in `agent/prompt_content/tests.rs`.
   - Proposed next slice: split raw tool input/output detail and sanitization helpers
     out of `agent/tool_details.rs` into a focused internal helper module while keeping
     the public tool-call event projection API and all redaction, path summary, command
     summary, and tool detail shapes unchanged.
   - Implementation status: tool-details I/O split is implemented and reviewed.
     `agent/tool_details.rs` remains the tool-call event projection facade, while
     `agent/tool_details_io.rs` owns raw input/output detail projection, command and
     path summaries, sensitive field redaction, and preview truncation.
   - Proposed next slice: split ACP session capability and authentication helpers out
     of `agent/acp_session_lifecycle.rs` into a focused internal module. Session
     lifecycle keeps side-effecting session new/load/list/close/delete behavior; the
     new helper owns initialize capability predicates, auth method validation, protocol
     validation, and auth retry method selection.
   - Implementation status: ACP session capabilities split is implemented and
     reviewed. `agent/acp_session_capabilities.rs` now owns pure initialize
     capability, protocol, auth validation, and auth retry helper logic, while
     `agent/acp_session_lifecycle.rs` keeps side-effecting session lifecycle
     operations.
   - Proposed next slice: split the ACP session client and command interface out of
     `agent/acp_session_worker.rs` into `agent/acp_session_client.rs`. The new module
     owns `AcpSessionClient`, command/input/startup result types, and stopped-worker
     terminal error presentation. `agent/acp_session_worker.rs` keeps the live ACP
     worker loop, session start/load, prompt dispatch, active update reading, close,
     delete, and config catalog delivery behavior.
   - Implementation status: ACP session client split is implemented and reviewed.
     `agent/acp_session_client.rs` now owns the channel-facing session handle,
     command/input/startup result types, and stopped-worker error presentation, while
     `agent/acp_session_worker.rs` is below the production source-size limit and keeps
     the live ACP worker loop and session I/O behavior.
   - Proposed next slice: split ACP probe and authentication execution out of
     `agent/acp_runtime_kernel.rs` into `agent/acp_probe_auth.rs`. The new module owns
     temporary ACP probe/auth connection execution, host capability handlers needed
     for authentication, timeout wrapping, initialize/auth validation calls, and ACP
     error mapping. `AcpRuntimeKernel` keeps registry lookup, public facade methods,
     auth cache updates, options-session state, active-session state, and shutdown.
     If `agent/acp_runtime_kernel.rs` remains oversized after this slice, the next
     kernel responsibility to grill is ACP options-session lifecycle and retry.
   - Implementation status: ACP probe/auth split is implemented and reviewed.
     `agent/acp_probe_auth.rs` now owns temporary probe/auth ACP connection execution,
     while `agent/acp_runtime_kernel.rs` keeps stateful runtime orchestration and
     remains an oversized split target.
   - Proposed next slice: split ACP options-session lifecycle and retry out of
     `agent/acp_runtime_kernel.rs` into `agent/acp_options_session_manager.rs`, with
     shared async-thread runtime bridging in `agent/acp_runtime_threading.rs`.
     `agent/acp_options_session.rs` keeps the live options worker protocol, while the
     new manager owns active options-session reuse, generation invalidation, retry,
     worker spawning, startup timeout handling, and shutdown close-task extraction.
   - Implementation status: ACP options-session manager split is implemented and
     reviewed. `agent/acp_options_session_manager.rs` now owns prepared options
     session lifecycle and retry, `agent/acp_options_session.rs` owns the live
     options worker protocol, `agent/acp_auth_method_cache.rs` owns the typed
     preferred-auth-method cache boundary, `agent/acp_session_paths.rs` owns session
     cwd normalization, and `agent/acp_runtime_threading.rs` owns generic runtime
     and close-task threading helpers.
   - Proposed next slice: split oversized protocol render model definitions from
     `protocol/model.rs` into a `protocol/model/` module tree grouped by Task,
     Chat, Activity, Permission, and Agent records. Keep `crate::protocol::model::*`
     as the stable Rust namespace and do not change serde protocol shapes, helper
     behavior, generated TypeScript semantics, or runtime behavior.
   - Implementation status: protocol model split is implemented and reviewed.
     `protocol/model/mod.rs` now preserves the stable `protocol::model` namespace
     while focused submodules own Task, Chat, Activity, Permission, and Agent model
     records. The previous oversized `protocol/model.rs` file has been removed.
   - Proposed next slice: split pure prompt attachment URI and resource naming
     helpers from near-limit `agent/prompt_content.rs` into
     `agent/prompt_content_uri.rs`. Keep prompt capability decisions, fallback
     behavior, ACP `ContentBlock` shapes, URI/path normalization behavior, and
     user-facing error text unchanged.
   - Implementation status: prompt-content URI helper split is implemented and
     reviewed. `agent/prompt_content_uri.rs` now owns attachment resource naming,
     file URI normalization, embedded attachment URI generation, URI-scheme
     detection, platform path detection, and percent encoding. `agent/prompt_content.rs`
     keeps prompt block construction, validation, payload classification, capability
     decisions, and error construction.
   - Proposed next slice: split active ACP task-session registry and worker spawning
     from `agent/acp_runtime_kernel.rs` into `agent/acp_active_session_manager.rs`.
     Keep worker behavior, prompt/cancel/close/delete dispatch, startup timeout
     behavior, duplicate-session protection, trace behavior, auth-method cache
     semantics, and shutdown close ordering unchanged.
   - Implementation status: ACP active-session manager split is implemented and
     reviewed. `agent/acp_active_session_manager.rs` owns active task-session
     registry state, start/load worker spawning, resume, event-sink attach,
     prompt/cancel/close/delete dispatch, duplicate active-session cleanup,
     startup timeout handling, and shutdown close-task extraction.
     `agent/acp_runtime_kernel.rs` now delegates active-session operations while
     retaining probe/auth, options-session, registry, host bridge, and auth-cache
     coordination.
   - Proposed next slice: split ACP client/host wiring out of
     `agent/acp_session_worker.rs`. Keep worker input contracts, start/load
     opening, command-loop behavior, session config catalog delivery, tracing,
     load replay capture, and all host capability behavior unchanged.
   - Accepted contract: create `agent/acp_session_connection.rs` with one
     worker-facing connection interface that wires the ACP `Client` builder,
     `session/update` load-replay interception, trace recording, and
     Agent-initiated host capability request handlers. Keep session lifecycle,
     initialization, start/load, prompt/cancel/close/delete, and config catalog
     delivery in `agent/acp_session_worker.rs`.
   - Implementation status: ACP session worker client/host wiring split is
     implemented and reviewed. `agent/acp_session_connection.rs` now owns ACP
     client construction, `session/update` interception, load-replay capture,
     and Agent-initiated host capability handler registration. Focused tests
     cover replay capture, nonmatching replay notifications, permission
     registration, and all filesystem and terminal host handler registrations.
   - Proposed next slice: split raw ACP session request I/O helpers out of
     `agent/acp_session_lifecycle.rs` into a focused
     `agent/acp_session_requests.rs` module. Keep start/load lifecycle
     orchestration, active-session attachment, load-replay projection,
     session-list result normalization, close/delete helpers, trace event names,
     and auth retry behavior unchanged.
   - Accepted contract: create `agent/acp_session_requests.rs` for
     `session/new`, `session/load`, and `session/list` request construction,
     blocking sends, AuthRequired retry, and existing new/load trace recording.
     Keep lifecycle capability validation, active-session attachment, replay
     capture/projection, config-option normalization, session-list result
     filtering, close/delete helpers, and product error mapping in
     `agent/acp_session_lifecycle.rs`.
   - Implementation status: ACP session request I/O split is implemented and
     reviewed. `agent/acp_session_requests.rs` now owns raw `session/new`,
     `session/load`, and `session/list` request I/O, AuthRequired retry, and
     existing new/load trace recording. `agent/acp_session_lifecycle.rs` keeps
     lifecycle orchestration, capability validation, active-session attachment,
     replay projection, listed-session filtering, close/delete helpers, and
     product error mapping.
   - Proposed next slice: split Agent catalog parsing, built-in definition
     construction, and inline registry tests out of `agent/registry.rs`. Keep
     runtime-facing `AgentRegistry`, `AgentDefinition`, `AgentLaunch`,
     `AgentSourceKind`, public Agent ids/labels, existing import paths, built-in
     launch policies, catalog validation behavior, task-create validation, and
     user-facing errors unchanged.
   - Accepted contract: keep `agent/registry.rs` as the runtime-facing facade
     for `AgentRegistry`, `AgentDefinition`, `AgentLaunch`, `AgentSourceKind`,
     Agent ids/labels, runtime lookup/validation, and the existing
     `AgentCatalogRecord` import path. Move catalog input normalization and
     record-to-definition conversion into `agent/registry_catalog.rs`, built-in
     definition and known built-in override construction into
     `agent/registry_builtin.rs`, and focused registry tests into
     `agent/registry/tests.rs`. Preserve all registry behavior and errors.
   - Implementation status: Agent Registry split is implemented and reviewed.
     `agent/registry.rs` remains the runtime-facing facade and re-exports
     `AgentCatalogRecord` from the existing path. `agent/registry_catalog.rs`
     owns catalog input normalization and conversion, `agent/registry_builtin.rs`
     owns built-in definition and known built-in override construction, and
     focused tests live in `agent/registry/tests.rs`.
   - Implementation status: App Server-owned Agent catalog startup storage is
     implemented. `agent/catalog_store.rs` loads `agents/catalog.json` from the
     protected App Server state root, applies stored custom Agent records and
     built-in enable/disable overrides over default built-ins, and both runtime
     startup paths now build `AgentRegistry` from the opened `Store` instead of
     a VS Code-provided environment variable. The VS Code runtime launcher no
     longer injects an Agent catalog env var. The obsolete VS Code custom Agent
     catalog collection and mutation stubs have been deleted; App Server
     protocol methods own Custom Agent mutations.
   - Implementation status: A6 custom Agent mutation protocol is implemented
     at the App Server boundary. `agent/saveCustom`, `agent/deleteCustom`, and
     `agent/setEnabled` are typed protocol methods with generated TypeScript
     bindings. Mutations write `agents/catalog.json`, replace the live shared
     `AgentRegistry` handle used by snapshots, ACP runtime, and task creation,
     and publish `AgentCollectionUpdated` events. Saving without `agentId`
     creates a new custom Agent; saving with an existing custom `agentId`
     replaces that catalog record under the same identity for the current
     metadata-edit path. Secret values are not stored in the catalog; only
     secret env names are persisted for launch-time shell requests.
   - Implementation status: Frontend Settings custom Agent save/delete/enable
     intents now prefer typed App Server Protocol requests through
     `BackendConnection`. Successful mutation responses update the visible
     Settings Agent rows and New Task Agent list immediately; rejected typed
     requests surface as Settings errors without replaying the legacy
     shell-owned mutation path. Legacy host mutation fallback has been removed.
   - Prior gap: App Server Agent collection snapshots were summary-only, and
     reloadable Custom Agent Settings details such as command line, icon, and
     env rows needed an App Server-owned Settings/Agent details read API before
     shell-owned Settings snapshots could disappear.
   - Implementation status: App Server-owned Agent Settings details reads are
     implemented through typed `settings/getAgentDetails`. The method projects
     known built-ins plus persisted custom Agent catalog records, including
     enabled state, status, icon, exact original command line, parsed command
     and args, safe env metadata, description, and render-ready capabilities.
     Custom Agent save persists icon and original command line alongside
     parsed launch fields. Frontend Settings refresh uses the typed read path
     when a `BackendConnection` is available and maps protocol detail rows into
     Agent-only Settings state. The stale VS Code full Settings snapshot
     collector has been removed; non-Agent Settings sections now need an
     App Server-owned replacement shape rather than a shell snapshot fallback.
     First replacement slice is implemented: App Server Protocol
     `SettingsSnapshot` now carries optional Backend-owned runtime/developer
     settings, and the protocol-edge startup path wires the live
     `RuntimeSettingsService` into initial and subscribed Settings snapshots.
     Follow-up audit after Custom Agent replacement cleanup found the active
     Frontend still keeps a dead legacy `settings:result` / `SettingsSnapshot`
     reducer path even though shell settings snapshots are no longer emitted.
     That stale Frontend settings snapshot state has been removed. Shared
     Frontend Settings state now keeps typed Agent details, controller-owned
     app preferences, and Backend runtime settings as separate projections;
     Settings rendering no longer imports or falls back to the transitional
     shell `SettingsSnapshot`.
     Follow-up audit selected deletion of the now-unused transitional shell
     `SettingsSnapshot` contract and unreachable MCP/Skills Settings panel
     components as the next cleanup packet. The shell contract
     cleanup is now implemented: `SettingsSnapshot`, shell-only
     common/developer Settings records, and the unreachable MCP and Skills
     panel components are deleted while still-used diagnostics, workspace-root,
     Agent, and Skill record types remain. Follow-up audit selected hiding
     unavailable MCP/Skills Settings tabs as the next cleanup because rendering
     endless skeletons for sections without App Server-owned projections is not
     an honest responsive UI state. That UI cleanup is implemented: current
     Settings renders only Agents and General, and stale unavailable tab state
     falls back to Agents until typed App Server projections exist. Follow-up
     audit found `SettingsSnapshot.sections` still advertises MCP/Skills
     despite missing projections; the next cleanup aligns advertised Backend
     section availability with currently renderable Settings sections. That
     cleanup was implemented first by advertising only Agents and Common
     Settings while MCP/Skills projection APIs were missing. Follow-up audit selected the
     non-Agent Settings projections API as the next real design packet before
     implementation.
     Accepted non-Agent Settings projection API: App Server adds typed read
     methods `settings/getMcpServers` and `settings/getSkills`. These methods
     return App Server-owned render records, not shell snapshots, and expose
     only safe labels, scopes, status, counts, descriptions, warnings, and
     generated timestamps. They must not expose raw local paths, secret values,
     shell picker metadata, or VS Code APIs. MCP and Skills are read-only in
     this slice; enable/edit/install mutations require later accepted APIs.
     `SettingsSnapshot.sections` advertises `mcpServers` and `skills` once their
     typed read methods exist; section presence means the view is reachable,
     while each result's typed availability says whether discovery is real. Frontend
     loads those projections through the central Settings intent layer on
     Settings startup and refresh, renders explicit loading/error/empty states,
     and does not infer section availability from shell kind or local paths. Skills
     scanning moves to Backend/App Server ownership using Backend-known global
     and project/workspace skill roots with safe source labels; any legacy VS
     Code scanner becomes either deleted code or a Backend-compatible helper,
     not an App Shell product source. MCP projection reads from Backend-owned
     MCP configuration/state when that source exists. When no discovery source
     exists, MCP and Skills methods return a typed `unavailable` projection with
     no records, and Frontend explains that discovery is unavailable instead of
     presenting the list as authoritatively empty or requiring a shell fallback.
     The protocol-source slice is implemented:
     `settings/getMcpServers` and `settings/getSkills` now have Rust protocol
     source types, method constants, generated TypeScript request/response
     maps, response aliases, and safe projection record types. Runtime routing,
     source-owned availability projections, Settings section advertisement, Frontend
     consumption, and responsive MCP/Skills tab rendering are implemented.
     The dead VS Code shell-owned Skills scanner has been deleted.
   - Implementation status: launch-affecting Custom Agent edits use the
     confirmed `agent/replaceCustom` mutation. Metadata-only edits use
     `agent/updateCustomMetadata` and cannot carry launch fields. App Server
     compares submitted launch fields with the stored custom Agent record,
     requires explicit `acceptedLaunchIdentityChange`, generates a new custom
     Agent id, removes mutable catalog/overlay/cache state for the old identity,
     returns typed cleanup metadata, and leaves historical Tasks renderable
     under their recorded old Agent id/label instead of rewriting Task history.
     Backend tests cover cleanup metadata and cached status clearing.
   - Proposed next slice: split task commit transaction orchestration and
     persistence helpers out of `tasks/mutation.rs` into a focused internal
     commit module. Keep the `TaskMutations` caller-facing API, mutation result
     types, lock semantics, message rollback behavior, revision assignment,
     notification timing, response snapshots, task-create validation timing, and
     existing tests unchanged.
   - Accepted contract: create `tasks/mutation/commit.rs` for lock-scoped
     existing-task and create-task commit flows, message backup/restore
     coordination, invariant validation, revision assignment, durable task
     persistence, notification publication, and optional response snapshots.
     Keep `tasks/mutation.rs` as the caller-facing facade for `TaskMutations`,
     `TaskMutationContext`, commit result/option types, and public entry-point
     methods. Do not add a separate dependency bundle or invariant module in
     this slice.
   - Implementation status: Task Mutation commit boundary split is implemented
     and reviewed. `tasks/mutation/commit.rs` owns existing-task and create-task
     commit transaction mechanics, message rollback, invariant validation,
     revision assignment, persistence, notification publication, and response
     snapshots. `tasks/mutation.rs` remains the caller-facing mutation facade.
   - Proposed next slice: split activity and permission message mutation helpers
     out of `storage/message_store.rs` into a focused child module. Keep the
     existing `Store` method signatures, message file layout, message version
     behavior, pagination behavior, activity finishing semantics, permission
     resolution errors, pending permission cancellation semantics, and tests
     unchanged.
   - Accepted contract: create `storage/message_store/mutations.rs` for
     existing-message rewrite operations: finishing the latest running activity,
     resolving permission messages, canceling pending permission messages, and
     permission decision validation. Keep `storage/message_store.rs` responsible
     for message file backup/restore, append/upsert, read, pagination, low-level
     writes, metadata, and version helpers. Preserve all existing `Store`
     method signatures and storage behavior.
   - Implementation status: Message Store mutation split is implemented and
     reviewed. `storage/message_store/mutations.rs` owns activity and permission
     existing-message rewrites, while `storage/message_store.rs` keeps message
     file persistence, pagination, metadata, and version helpers. Existing
     `Store` method signatures and storage behavior are unchanged.
   - Proposed next slice: split follow-up prompt orchestration out of
     `tasks/turn_lifecycle.rs` into `tasks/turn_lifecycle/prompt.rs`. Keep
     create/adopt flows, cancel, permission response, shared lifecycle helpers,
     prompt validation, active-turn checks, Agent session start/resume behavior,
     event attachment timing, commit behavior, response snapshots, and turn
     spawn guard unchanged.
   - Accepted contract: create `tasks/turn_lifecycle/prompt.rs` for
     `TaskTurnLifecycle::prompt`, prompt-only `AgentSessionPlan`, prompt session
     start/resume orchestration, event attachment, chat commit, close-on-failure
     handling, response snapshot mapping, and guarded turn spawn. Keep
     `tasks/turn_lifecycle.rs` as the lifecycle facade and keep create/adopt,
     cancel, permission response, shared helpers, `snapshot_chat_commit_options`,
     and `required_prompt_text` there.
   - Implementation status: Task Turn prompt split is implemented and reviewed.
     `tasks/turn_lifecycle/prompt.rs` owns `TaskTurnLifecycle::prompt`,
     prompt-only session planning, Agent session start/resume orchestration,
     event attachment, chat commit, close-on-failure handling, response snapshot
     mapping, and guarded turn spawn. `tasks/turn_lifecycle.rs` remains the
     lifecycle facade and shared helpers remain private.
   - Proposed next slice: split Task Turn event-sink internals out of
     `tasks/turn_events.rs`. Keep `TaskEventSink`, `TaskSessionEventSink`,
     `PermissionWaiters`, Agent event routing, message normalization behavior,
     mutation commit semantics, unread/status updates, and current Task
     lifecycle imports unchanged. Move streaming text/thought run accumulation,
     permission waiter state, and config-option commit helpers into focused
     child modules. Do not migrate permission requests to `server_requests`,
     change App Server Protocol publication, or touch Agent ACP I/O in this
     slice.
   - Accepted contract: keep `tasks/turn_events.rs` as the caller-facing
     event-sink facade for `TaskEventSink`, `TaskSessionEventSink`,
     `PermissionWaiters`, Agent event routing, message append/upsert commits,
     permission request orchestration, and existing Task lifecycle imports.
     Move streamed text/thought run accumulation into
     `tasks/turn_events/streaming.rs`, permission waiter state and
     cancellation-aware waiting into `tasks/turn_events/permissions.rs`, and
     config-option commit projection into `tasks/turn_events/config.rs`.
     Preserve normalization output, streaming run clear rules, tool-call
     `scope_id` defaults, active-turn/cancellation guards, unread/status
     updates, config projection, and permission waiter cleanup behavior.
   - Implementation status: Task Turn event-sink split is implemented and
     reviewed. `tasks/turn_events.rs` remains the event-sink facade, while
     `tasks/turn_events/streaming.rs` owns text/thought run accumulation,
     `tasks/turn_events/permissions.rs` owns permission waiter registry state,
     cancellation-aware waiting, and response routing, and
     `tasks/turn_events/config.rs` owns config-option commit projection.
   - Proposed next slice: split tool-detail sanitization and redaction policy
     out of `agent/tool_details_io.rs` into a focused helper module. Keep
     `tool_details_io.rs` as the raw ACP tool input/output projection facade and
     preserve redaction behavior, path leaf summaries, command summaries,
     scalar field summaries, preview truncation, field ordering, and
     `ActivityTool*` protocol shapes unchanged.
   - Accepted contract: create `agent/tool_details_sanitizer.rs` for display
     safety policy: preview truncation, sensitive-key classification, scalar and
     command summary sanitization, command-array normalization, shell launcher
     detection, path leaf summaries, path-like detection, and per-field summary
     classification. Keep `agent/tool_details_io.rs` responsible for raw JSON
     parsing, top-level input/output field selection, excluded-field handling,
     extra scalar field ordering and limiting, and `ActivityToolInput` /
     `ActivityToolOutput` construction. Keep sanitizer helpers no wider than
     `pub(super)` and preserve all current redaction and protocol behavior.
   - Implementation status: Tool Details sanitization split is implemented and
     reviewed. `agent/tool_details_sanitizer.rs` owns display safety and
     preview policy as a private Agent module, while `agent/tool_details_io.rs`
     remains the raw input/output projection facade and continues to expose the
     existing helper surface to `agent/tool_details.rs`. Review fixes added
     product-level truncation coverage and narrowed sanitizer module visibility.
   - Proposed next slice: split the ACP options-session command client out of
     `agent/acp_options_session.rs` into
     `agent/acp_options_session_client.rs`. The new module owns
     `AcpOptionsSessionClient`, command channel construction, the command
     receiver wrapper, the command enum shared with the worker, and
     stopped-worker/timeout error mapping for client methods. Keep the live ACP
     options worker, startup, invalidation, catalog projection, command loop,
     list-session dispatch, and close behavior in `agent/acp_options_session.rs`
     unchanged.
   - Accepted contract: create `agent/acp_options_session_client.rs` for
     `AcpOptionsSessionClient`, `AcpOptionsCommandReceiver`,
     `AcpOptionsCommand`, `options_session_channel`, synchronous client methods,
     request/reply setup, stopped-worker error mapping, and existing
     list-session/close timeout behavior. Keep `agent/acp_options_session.rs`
     responsible for `AcpOptionsSessionWorkerInput`, ACP connection startup,
     permission invalidation, catalog projection, command execution, set-option
     application, list-session dispatch, close execution, and `acp_error`
     mapping. Manager policy must change only by import path.
   - Implementation status: ACP Options Session client split is implemented and
     reviewed. `agent/acp_options_session_client.rs` owns the channel-facing
     client and command types, while `agent/acp_options_session.rs` keeps the
     live ACP worker and command execution behavior. Manager lifecycle policy is
     unchanged except for import paths.
   - Proposed next slice: split ACP session opening out of
     `agent/acp_session_worker.rs` into `agent/acp_session_opening.rs`. The new
     module owns initialize tracing, `AcpSessionRunner` construction,
     prompt-content policy derivation, start/load opening, prompt attachment
     validation, config-option application, close-on-apply-failure behavior,
     load replay capture, startup error reporting, and opened-session result
     construction. Keep the live worker command loop, prompt dispatch,
     close/delete handling, session config catalog delivery, active update
     reading, and final `acp_error` mapping in `agent/acp_session_worker.rs`.
   - Accepted contract: create `agent/acp_session_opening.rs` with one
     worker-facing opening function returning an opened active session,
     `AcpSessionRunner`, `supports_session_close`, `PromptContentPolicy`,
     startup `AgentSession`, and replayed messages. The opening module owns
     initialize request tracing, `initialize_agent_connection`, start/load
     opening, prompt attachment validation, config-option application,
     close-on-apply-failure behavior, load replay capture, and startup error
     reporting. `agent/acp_session_worker.rs` keeps worker input contracts,
     ACP connection setup, startup success send, live command/update loop,
     prompt dispatch, close/delete handling, session config catalog delivery,
     and final `acp_error` mapping.
   - Implementation status: ACP Session opening split is implemented, reviewed,
     and verified. `agent/acp_session_opening.rs` owns opening/startup behavior,
     while `agent/acp_session_worker.rs` keeps the live command/update loop and
     startup success reporting.
   - Review status: `$doomsday-review` correctness, requirements/tests, and
     code-quality passes completed with no findings.
   - Verification status: Frontend Host Message Router split passed focused
     Frontend checks, public router tests, repo-wide check, repo-wide test,
     JSON validation, diff whitespace check, boundary scan, and source-size
     scan.
   - Proposed next slice: split `appControllerCallbacks.ts` into focused
     navigation, settings, new-task, and task callback modules while keeping
     `createAppCallbacks` as the public user-intent seam. Preserve
     dispatch-before-host-message ordering, request id generation, guard paths,
     host message payloads, and callback tests.
   - Accepted contract: keep `createAppCallbacks` plus the callback group types
     importable from `appControllerCallbacks.ts`; extract private callback-group
     modules for navigation, settings, new-task, and task commands. Preserve
     archive toggles, native-session adoption, settings optimistic local
     updates, config-option mutation, new-task submit, active-task prompt,
     permission response, tool-detail cache guards, missing-snapshot no-ops, and
     existing `AppSurfaces` callback wiring.
   - Implementation status: Frontend App Controller Callbacks split is
     implemented and ready for review. `appControllerCallbacks.ts` remains the
     public factory and type export module; focused callback modules now own
     navigation, settings, new-task, and task intent groups. Public factory
     tests cover the accepted ordering and guard invariants.
   - Review status: `$doomsday-review` correctness, requirements/tests, and
     code-quality passes completed after adding explicit cross-mock ordering
     assertions and `AppSurfaces` callback wiring coverage; final rerun
     reported no findings.
   - Verification status: Frontend App Controller Callbacks split passed
     focused Frontend checks, public callback and surface wiring tests,
     repo-wide check, repo-wide test, diff whitespace check, boundary scan, and
     source-size scan.

5. **Frontend Split**
   - Move shared UI into shell-neutral Frontend.
   - Define shell injection points for navigation composition, routing, commands, pickers, and shell capabilities.
   - Remove product workflow decisions from Frontend where they belong to App Server.
   - Proposed next slice: split the oversized Settings page into local
     shell-neutral Settings components. Keep `SettingsView.tsx` as the page
     shell and tab router, move Agent, MCP, Skills, and General tab renderers
     into focused modules, and move shared settings presentation helpers into a
     local helper module. Preserve settings state shape, callback props, CSS
     classes, ARIA attributes, visible text, and host/runtime contracts.
   - Accepted contract: keep `SettingsView.tsx` as the public page component
     with the same props, tab keyboard navigation, developer unlock counter, and
     loading/error/snapshot surface. Move Agent, MCP, Skills, and General tabs
     into focused local modules and move shared settings helpers into a
     presentation-only helper module. Tab components receive typed render data
     and callbacks only; they must not import host bridge functions, App Server
     clients, reducer actions, or shell/runtime services.
   - Implementation status: Frontend Settings split is implemented, reviewed,
     and verified. `SettingsView.tsx` is now the page shell and tab router,
     while `AgentSettingsTab.tsx`, `GeneralSettingsTab.tsx`, and
     `settingsPresentation.tsx` own focused local Settings UI responsibilities.
     The earlier MCP and Skills tab components were later removed when the
     legacy full shell Settings snapshot path was deleted; those tabs now wait
     for App Server-owned projections.
   - Review status: `$doomsday-review` correctness, requirements/tests, and
     code-quality passes completed with no findings.
   - Proposed next slice: split `ChatMessageView.tsx` into focused
     shell-neutral Chat message components. Keep `ChatMessageView.tsx` as the
     public `ChatRow` message-kind router, move copy actions, activity/tool
     rendering, tool path opening, and permission-card rendering into local
     modules, and preserve visible text, CSS classes, ARIA labels, lazy
     tool-detail loading, `firstToolPath` re-export, and permission response
     behavior.
   - Accepted contract: keep `ChatMessageView.tsx` as the public `ChatRow`
     component and message-kind router with the same props and
     `firstToolPath` re-export. Move copy actions, activity row rendering,
     tool-detail rendering, reusable tool-detail blocks, and permission-card
     rendering into focused local modules. Only the tool-path component may
     import `postHostMessage`, and only for the existing `tool.openPath` action.
   - Implementation status: Frontend Chat Message split is implemented,
     reviewed, and verified. `ChatMessageView.tsx` is now the public row
     router, while `chatMessageActions.tsx`, `ChatActivityView.tsx`,
     `ChatToolDetailsView.tsx`, `ChatToolBlocks.tsx`,
     `ChatPermissionCard.tsx`, and `chatToolIcons.tsx` own focused local Chat
     message UI responsibilities.
   - Review status: `$doomsday-review` correctness, requirements/tests, and
     code-quality passes completed after fixes with no findings.
   - Proposed next slice: split `appReducer.ts` into focused reducer-domain
     modules while keeping `appReducer.ts` as the public `AppAction` type owner
     and central dispatch entry point. Move new-task, task interaction/chat/tool
     detail/permission, and Settings reducer logic into local state modules.
     Preserve action names, payload types, `AppState` shape, optimistic/pending
     behavior, and tests.
   - Accepted contract: keep `appReducer.ts` as the only public `AppAction` and
     `appReducer` export. Add focused `newTaskReducer.ts`,
     `taskInteractionReducer.ts`, and `settingsReducer.ts` modules that receive
     full `AppState` plus `AppAction` and return `AppState | undefined`.
     Preserve all existing action names, payload types, state shape,
     optimistic/pending behavior, and reducer tests.
   - Implementation status: Frontend reducer-domain split is implemented and
     ready for review. `appReducer.ts` remains the public action and reducer
     entry point, while `newTaskReducer.ts`, `taskInteractionReducer.ts`, and
     `settingsReducer.ts` own focused state-domain reducer logic.
   - Review status: `$doomsday-review` correctness, requirements/tests, and
     code-quality passes completed after fixing one reducer-boundary export;
     targeted rerun reported no findings.
   - Verification status: Frontend reducer-domain split passed focused
     Frontend checks, reducer tests, repo-wide check, JSON validation, diff
     whitespace check, boundary scan, and source-size scan.
   - Proposed next slice: split `App.tsx` into focused Frontend controller
     modules while keeping `App.tsx` as the public root component. Move
     bootstrap/session startup effects, surface-specific host callbacks, and
     surface rendering branches into local shell-neutral controller modules or
     render helpers. Preserve host message contracts, telemetry names, snapshot
     request behavior, polling cadence, reducer actions, visible UI text, and
     responsive pending/optimistic behavior.
   - Accepted contract: keep `App.tsx` as the only public `App` root and
     preserve existing `App.tsx` re-exports. Add `appController.ts` with
     `useAppController()` to own bootstrap, reducer setup, host message session
     startup, snapshot request refs, request de-duplication refs, startup
     requests, polling, telemetry, config-option loading, native-session
     loading, and typed callback groups. Add `AppSurfaces.tsx` to own the
     surface switch and receive render data plus callbacks only. Preserve host
     message contracts, telemetry names, snapshot behavior, polling cadence,
     reducer actions, visible UI text, class names, ARIA labels, and responsive
     pending behavior.
   - Implementation status: Frontend App controller split is implemented and
     ready for review. `App.tsx` is now the public root wrapper,
     `appController.ts` owns lifecycle effects, `appControllerCallbacks.ts`
     owns host-aware callback groups, and `AppSurfaces.tsx` owns surface
     rendering. `NewTaskView.tsx` and `TaskView.tsx` now receive typed host
     callbacks instead of importing host bridge APIs directly.
   - Review status: `$doomsday-review` correctness, requirements/tests, and
     code-quality passes completed after adding controller callback, lifecycle
     helper, and mounted hook coverage; targeted rerun reported no findings.
   - Verification status: Frontend App controller split passed focused
     Frontend checks, mounted controller and callback tests, repo-wide check,
     JSON validation, diff whitespace check, boundary scan, and source-size
     scan.
   - Proposed next slice: split `AgentSettingsTab.tsx` into focused Settings
     subcomponents while keeping `AgentSettingsTab.tsx` as the public Settings
     tab component and local state owner. Move Agent list, detail
     header/status, custom Agent draft form, icon picker, environment editor,
     and pure draft/acknowledgement helpers into local Settings modules.
     Preserve visible text, CSS class names, ARIA labels, callback props,
     custom Agent save/delete payloads, authentication behavior, and Settings
     tests.
   - Accepted contract: keep `AgentSettingsTab.tsx` as the public Settings tab
     component, with the same props and acknowledgement helper import path.
     Keep selected Agent, custom Agent draft, delete confirmation, pending save,
     and pending delete state in `AgentSettingsTab.tsx`. Extract pure draft and
     acknowledgement helpers, the Agent list, Agent detail/status/launch/
     availability rendering, custom Agent icon picker, and custom Agent
     environment editor into shell-neutral local Settings modules. Preserve save
     and delete acknowledgement semantics, custom Agent save/delete payloads,
     built-in Agent enable behavior, authentication button behavior, visible
     text, class names, roles, ARIA labels, and Settings tests.
   - Implementation status: Frontend Agent Settings tab split is implemented
     and ready for review. `AgentSettingsTab.tsx` remains the state-owning
     public tab, while `agentSettingsModel.ts`, `AgentSettingsList.tsx`,
     `AgentSettingsDetail.tsx`, and `AgentCustomFields.tsx` own focused helper
     and presentation responsibilities. Mounted tests now cover custom Agent
     save payloads, delete confirmation, and built-in Agent enable toggles.
   - Review status: `$doomsday-review` correctness, requirements/tests, and
     code-quality passes completed after fixing an empty-Agent visible text
     regression and duplicated primary authentication selection; targeted
     reruns reported no findings.
   - Verification status: Frontend Agent Settings tab split passed focused
     Frontend checks, mounted Settings tests, repo-wide check, JSON validation,
     diff whitespace check, boundary scan, and source-size scan.
   - Proposed next slice: split `Composer.tsx` into focused shell-neutral
     composer modules while keeping `Composer.tsx` as the public Composer
     component and `shouldSubmitComposerKey` helper owner. Move attachment token
     rendering, menu primitives, selector/menu rendering, and config-option
     label/icon helpers into local modules. Preserve Task/New Task call sites,
     visible text, class names, roles, ARIA labels, icons, locked/disabled
     behavior, Escape menu closing, submit shortcut behavior, and Composer
     tests.
   - Accepted contract: keep `Composer.tsx` as the public Composer component
     with the same props and `shouldSubmitComposerKey` import path. Keep
     `openMenu` state, Escape-to-close behavior, and textarea submit handling
     in `Composer.tsx`. Extract `composerKeymap.ts`,
     `ComposerAttachments.tsx`, local `ComposerPrimitives.tsx`, and
     `ComposerMenus.tsx` for shortcut logic, attachment tokens, local menu
     primitives, Agent/config/isolation/add-context menus, and config-option
     helper rendering. Preserve visible text, class names, roles, ARIA labels,
     titles, icons, locked/disabled behavior, menu callback behavior, submit
     shortcut behavior, and Composer tests.
   - Implementation status: Frontend Composer split is implemented and ready
     for review. `Composer.tsx` remains the public component and owns menu
     state, Escape close, and textarea submit handling; `composerKeymap.ts`,
     `ComposerAttachments.tsx`, `ComposerPrimitives.tsx`, and
     `ComposerMenus.tsx` own the extracted shortcut, attachment, primitive, and
     menu responsibilities.
   - Review status: `$doomsday-review` correctness, requirements/tests, and
     code-quality passes completed after adding mounted Composer behavior tests
     for extracted menu, attachment, disabled, locked, cancel/send, and
     keyboard-submit invariants; final rerun reported no findings.
   - Verification status: Frontend Composer split passed focused Frontend
     checks, mounted Composer tests, repo-wide check, repo-wide test, JSON
     validation, diff whitespace check, boundary scan, and source-size scan.
   - Proposed next slice: split `hostMessageRouter.ts` into focused domain
     routers while keeping `routeHostMessage` as the single public App
     Shell-message ingress for Frontend state ingestion. Preserve stale
     request filtering, snapshot telemetry, runtime error mapping, dispatch
     actions, posted shell messages, and existing host-message router tests.
   - Accepted contract: keep `routeHostMessage(message, context)` as the
     public App Shell-message ingress and keep `sendWebviewTelemetry`
     importable. Extract private domain routers for settings/catalog,
     Agent-option/native-session, navigation, Task/chat/tool, and runtime-error
     messages. Preserve current routing order, stale result guards, snapshot
     telemetry, pagination follow-up, dispatch actions, posted shell messages,
     fallback error text, and public-router tests.
   - Implementation status: Frontend Host Message Router split is implemented
     and ready for review. `hostMessageRouter.ts` remains the public ingress;
     focused state modules now own settings/catalog, Agent session/options,
     navigation, Task/chat/tool, telemetry, context types, and runtime-error
     routing. Public router tests cover the accepted stale-result, pagination,
     snapshot, and error-routing invariants.
   - Review status: `$doomsday-review` correctness, requirements/tests, and
     code-quality passes completed with no findings.
   - Verification status: Frontend Host Message Router split passed focused
     Frontend checks, public router tests, repo-wide check, repo-wide test,
     JSON validation, diff whitespace check, boundary scan, and source-size
     scan.
   - Proposed next slice: split `appControllerCallbacks.ts` into focused
     navigation, settings, new-task, and task callback modules while keeping
     `createAppCallbacks` as the public user-intent seam. Preserve
     dispatch-before-host-message ordering, request id generation, guard paths,
     host message payloads, and callback tests.
   - Accepted contract: keep `createAppCallbacks` plus the callback group types
     importable from `appControllerCallbacks.ts`; extract private callback-group
     modules for navigation, settings, new-task, and task commands. Preserve
     archive toggles, native-session adoption, settings optimistic local
     updates, config-option mutation, new-task submit, active-task prompt,
     permission response, tool-detail cache guards, missing-snapshot no-ops,
     and existing `AppSurfaces` callback wiring.
   - Implementation status: Frontend App Controller Callbacks split is
     implemented and ready for review. `appControllerCallbacks.ts` remains the
     public factory and type export module; focused callback modules now own
     navigation, settings, new-task, and task intent groups. Public factory
     tests cover the accepted ordering and guard invariants.
   - Review status: `$doomsday-review` correctness, requirements/tests, and
     code-quality passes completed after adding explicit cross-mock ordering
     assertions and `AppSurfaces` callback wiring coverage; final rerun
     reported no findings.
   - Verification status: Frontend App Controller Callbacks split passed
     focused Frontend checks, public callback and surface wiring tests,
     repo-wide check, repo-wide test, diff whitespace check, boundary scan, and
     source-size scan.
   - Proposed next slice: split the Frontend tool-details cluster into focused
     pure view-model modules and focused renderer modules while keeping
     `ChatToolDetails` as the public renderer, `ChatToolBlocks` exports stable,
     and compatibility helper exports available from their current import
     paths. Preserve loading/error/fallback behavior, read/edit/search/execute
     rendering, generic tool rendering, path-open host messages, CSS classes,
     ARIA labels, visible text, and tests.
   - Accepted contract: keep `ChatToolDetailsView.tsx` as the public
     tool-details router exporting `ChatToolDetails`. Split pure state helpers
     around generic details, command/output normalization, edit/diff helpers,
     search parsing/info, and execute output classification. Split concrete
     renderers into read, edit, search, execute, and generic modules. View-model
     modules stay shell-neutral and React-free; renderer modules depend only on
     React, icons, `ChatToolBlocks`, and pure helpers; host message posting
     remains isolated to `ToolPath`.
   - Implementation status: Frontend Tool Details split is implemented and
     ready for review. `ChatToolDetailsView.tsx` remains the public router,
     focused renderer modules own read, edit, search, execute, and generic tool
     details, and pure view-model modules own shared, command, edit, search,
     and execute helper responsibilities behind the existing compatibility
     import path.
   - Review status: `$doomsday-review` correctness, requirements/tests, and
     code-quality passes completed after fixing structured search file-result
     normalization and renderer-level file-list path coverage; targeted reruns
     reported no findings.
   - Verification status: Frontend Tool Details split passed focused Frontend
     checks, focused renderer/helper tests, repo-wide check, repo-wide test,
     JSON validation, diff whitespace check, boundary scan, and source-size
     scan.
   - Proposed next slice: split the standalone browser preview host in
     `services/devHost.ts` into focused shell-neutral demo data, standalone
     bootstrap, and message-routing modules while keeping `devHost.ts` as the
     public facade used by `hostBridge.ts`. Preserve demo data, path mapping,
     response metadata, async dispatch timing, route transitions, message
     payload shapes, and VS Code webview opt-out behavior.
   - Accepted contract: keep `standaloneBootstrap()` and
     `createStandaloneHost()` importable from `services/devHost.ts`. Extract
     `devHostData`, `devHostBootstrap`, and `devHostRouter` responsibilities so
     demo data is browser-free, bootstrap logic owns standalone detection and
     path mapping, and routing owns typed demo responses through injected
     posting/navigation functions. `devHost.ts` remains the only module that
     wires direct browser globals to the standalone preview host.
   - Implementation status: Frontend Standalone Dev Host split is implemented
     and ready for review. `devHost.ts` remains the public facade,
     `devHostBootstrap.ts` owns standalone path/bootstrap mapping,
     `devHostRouter.ts` owns typed message routing through injected outputs, and
     `devHostData.ts` owns browser-free demo data behind a narrow operation
     contract.
   - Review status: `$doomsday-review` correctness, requirements/tests, and
     code-quality passes completed after adding facade-level browser wiring
     coverage and tightening the demo data boundary; targeted reruns reported
     no findings.
   - Verification status: Frontend Standalone Dev Host split passed focused
     Frontend checks, focused dev-host tests, repo-wide check, repo-wide test,
     JSON validation, diff whitespace check, boundary scan, and source-size
     scan.
   - Proposed next slice: split `state/chatPaging.ts` into focused pure
     modules for page merging, item normalization/filtering, text and thought
     coalescing, and activity coalescing while keeping `chatPaging.ts` as the
     public state/view-model facade. Preserve message deduplication order,
     cursor propagation, pending/error state, legacy Thought conversion,
     filtered legacy Working rows, Agent text coalescing heuristics, and
     activity-run title/status behavior.
   - Accepted contract: keep `RenderedChat`, `mergePageState`, and
     `renderedChat` importable from `state/chatPaging.ts`. Extract pure
     shell-neutral modules for page merge, chat item normalization, text/thought
     coalescing, and activity coalescing. New modules must not import React,
     browser APIs, host bridge, reducers, App Server clients, timers, or mutate
     input arrays.
   - Implementation status: Frontend Chat Paging split is implemented and ready
     for review. `chatPaging.ts` remains the public facade, while
     `chatPageMerge.ts`, `chatItemNormalization.ts`, `chatTextCoalescing.ts`,
     and `chatActivityCoalescing.ts` own focused pure responsibilities behind
     it.
   - Review status: `$doomsday-review` correctness, requirements/tests, and
     code-quality passes completed after adding missing activity title
     classification coverage; targeted rerun reported no findings.
   - Verification status: Frontend Chat Paging split passed focused Frontend
     checks, focused chat paging/reducer tests, repo-wide check, repo-wide test
     after one unrelated transient backend test retry, JSON validation, diff
     whitespace check, boundary scan, and source-size scan.
   - Superseded behavior note: the module split remains useful, but the accepted
     adjacency-based text/thought/activity coalescing behavior did not survive
     real-session evidence. App Server now supplies stable Chat identities and
     chunks update those identities directly, so the Frontend facade preserves
     distinct rows and no longer invokes the heuristic coalescers.
   - Proposed next slice: split `components/Sidebar.tsx` into a public
     composition facade, pure sidebar view-model derivation, focused task-row
     rendering, and focused native-session row rendering while keeping the
     `Sidebar` prop contract unchanged. Preserve search/archive behavior,
     visible-count and empty-state copy, native-session adoption disabled
     states, pagination callback behavior, selected/read/status classes, and
     archive/restore/open labels.
   - Accepted contract: keep `Sidebar` importable from `components/Sidebar`.
     Extract shell-neutral view-model derivation for visible native sessions,
     visible count, and empty-state copy. Extract focused sidebar row component
     modules for task rows and native-session rows. Shared row metadata/action
     helpers stay local to sidebar components rather than becoming general
     abstractions. Extracted modules must not import App Server, transport,
     browser globals, timers, or mutable singleton state.
   - Implementation status: Frontend Sidebar split is implemented and ready for
     review. `Sidebar.tsx` remains the public facade, `sidebarViewModel.ts`
     owns pure list/count/empty-state derivation, `SidebarTaskRow.tsx` owns
     task row rendering, `SidebarNativeSessionRow.tsx` owns listed native
     session row rendering, and `SidebarRowParts.tsx` owns sidebar-local row
     helpers.
   - Review status: `$doomsday-review` correctness, requirements/tests, and
     code-quality passes completed after adding top-level Sidebar facade
     coverage and narrowing the view-model type boundary; targeted rerun
     reported no findings.
   - Verification status: Frontend Sidebar split passed focused Frontend
     checks, focused sidebar/AppSurfaces tests, repo-wide check, repo-wide
     test, JSON validation, diff whitespace check, boundary scan, and changed
     source-size scan.
   - Proposed next slice: split `components/appController.ts` into a public
     `useAppController` facade plus controller-local mutable refs, focused
     native-session request posting, and pure derived controller view state.
     Preserve bootstrap preference/agent initialization, snapshot request id
     and navigation invalidation behavior, native-session request id/cursor
     payload behavior, fallback agent selection, task-open fallback timer,
     active-task polling cadence, task-rendered telemetry fields, and visible
     task filtering semantics.
   - Accepted contract: keep `useAppController()` importable from
     `components/appController` and keep the exported `AppController` type
     shape unchanged. Extract controller mutable ref construction into a small
     hook, native-session request posting into an explicit-dependency helper,
     and active/visible task derivation into a pure shell-neutral module.
     `appController.ts` remains the only module that wires React effects,
     host-message session startup, timers, telemetry effects, reducer state,
     and callback factory assembly.
   - Implementation status: Frontend App Controller Assembly split is
     implemented and ready for review. `appController.ts` remains the public
     `useAppController` facade, `appControllerRefs.ts` owns controller-local
     mutable refs, `appControllerNativeSessions.ts` owns explicit-dependency
     native-session request posting, and `appControllerDerivedState.ts` owns
     pure active/visible task derivation.
   - Review status: `$doomsday-review` correctness, requirements/tests, and
     code-quality passes completed with no findings.
   - Verification status: Frontend App Controller Assembly split passed focused
     Frontend checks, focused controller tests, repo-wide check, repo-wide test,
     JSON validation, diff whitespace check, boundary scan, and changed
     source-size scan.

5.5. **Backend Transport Split**
   - Proposed next slice: split `transport/shell_control.rs` into a stable public
     `ShellControlDispatcher` facade plus focused JSON-RPC line/value handling and method
     routing helpers. Preserve host-response-first handling, batch response
     ordering, parse/invalid-request errors, notification behavior, shutdown
     gating, unknown-method errors, response serialization, and runtime service
     ownership.
   - Accepted contract: keep `transport::shell_control::ShellControlDispatcher`,
     `ShellControlDispatcher::new`, `ShellControlDispatcher::new_with_host`, `handle_line`, and
     `shutdown_requested` behavior unchanged. `ShellControlDispatcher` remains the owner of
     `Runtime`, `HostBridge`, and shutdown state. Extract method routing behind
     explicit runtime/shutdown dependencies, keep parse/serialization helpers
     transport-local, and keep host-response consumption before JSON-RPC request
     deserialization.
   - Implementation status: Backend Transport ShellControlDispatcher split is implemented
     and reviewed. `shell_control.rs` is the public `ShellControlDispatcher`
     facade, `shell_control/codec.rs` owns JSON line parsing and response
     serialization, and `shell_control/method_dispatch.rs` owns runtime method
     routing plus params/result conversion.
   - Review status: `$doomsday-review` correctness, requirements/tests, and
     code-quality passes completed after adding missing dispatcher-boundary
     coverage for invalid JSON, invalid versions, notification no-response plus
     failure logging, and unknown methods; targeted rerun reported no findings.
   - Verification status: Backend Transport ShellControlDispatcher split passed focused
     Rust format/check/tests, repo-wide check, repo-wide test, JSON validation,
     diff whitespace check, boundary scan, and changed source-size scan.
   - Proposed next slice: move the inline dispatcher tests from
     `transport/shell_control.rs` to `transport/shell_control/tests.rs` while preserving
     all dispatcher-boundary coverage for invalid JSON, invalid versions,
     notification no-response plus failure logging, unknown methods,
     host-response handling, pending host bridge responses, and shutdown
     rejection.
   - Accepted contract: keep `transport::shell_control::ShellControlDispatcher` and all public
     dispatcher behavior unchanged. Replace the inline test module with
     `#[cfg(test)] mod tests;`, keep tests at the public dispatcher boundary,
     and do not add production helpers solely for the test move.
   - Implementation status: Backend Transport ShellControlDispatcher test-layout split is
     implemented and ready for review. `dispatch.rs` contains production
     dispatcher code plus `#[cfg(test)] mod tests;`, while
     `dispatch/tests.rs` owns the dispatcher-boundary tests.
   - Review status: `$doomsday-review` correctness, requirements/tests, and
     code-quality passes completed with no findings.
   - Verification status: Backend Transport ShellControlDispatcher test-layout split passed
     focused Rust format/check/tests, repo-wide check, repo-wide test, JSON
     validation, diff whitespace check, inline-test scan, and changed
     source-size scan.
   - Proposed next slice: move the inline host bridge tests from
     `protocol/host.rs` to `protocol/host/tests.rs` while preserving public
     `HostBridge` request/response, disabled-bridge rejection, and cancellable
     `request_until` coverage.
   - Accepted contract: keep `protocol::host::HostBridge`, `HostRequest`, and
     all public host bridge behavior unchanged. Replace the inline test module
     with `#[cfg(test)] mod tests;`, keep tests at the public host bridge
     boundary, and do not add production helpers or widen visibility solely for
     the test move.
   - Implementation status: Backend Host Bridge test-layout split is
     implemented and ready for integration verification. `protocol/host.rs`
     contains production host bridge code plus `#[cfg(test)] mod tests;`, while
     `protocol/host/tests.rs` owns the host bridge boundary tests.
   - Review status: `$doomsday-review` correctness, requirements/tests, and
     code-quality passes completed with no findings.
   - Verification status: Backend Host Bridge test-layout split passed focused
     Rust format/check/tests, repo-wide check, repo-wide test, JSON validation,
     diff whitespace check, inline-test scan, and changed source-size scan.

6. **App Shells**
   - Implement Web, Desktop, and VS Code shells as thin shell owners around shared Frontend.
   - Each shell launches or connects to a compatible local App Server for the selected state root.
   - Shells use the same App Server Protocol product API.
   - Proposed next slice: split
     `packages/app-shell-contracts/src/runtimeTypes.ts` into focused runtime
     contract type modules while preserving the package root and
     `runtimeTypes.ts` export surface.
   - Accepted contract: keep `@openaide/app-shell-contracts` package exports
     and existing `./runtimeTypes.js` imports compatible. Convert
     `runtimeTypes.ts` into a facade over focused modules under
     `src/runtime/` for primitives, requests, chat, task, agent, and system
     result types. Keep all exported type names, method names, union members,
     field names, optionality, and structural shapes unchanged; do not add
     runtime behavior or validation logic in this slice.
   - Implementation status: App Shell Contracts runtime type split is
     implemented and ready for integration verification. `runtimeTypes.ts`
     remains the compatibility facade, while `src/runtime/` owns primitives,
     chat, task, agent, request-map, and system/result-map types.
   - Review status: `$doomsday-review` correctness, requirements/tests, and
     code-quality passes completed after fixing a whitespace verification
     finding; targeted requirements/tests rerun reported no findings.
   - Verification status: App Shell Contracts runtime type split passed
     focused package check/build, repo-wide check, repo-wide test, JSON
     validation, diff whitespace check, exported type-name compatibility diff,
     and changed source-size scan.
   - Proposed next slice: split
     `packages/app-shell-contracts/src/webviewTypes.ts` into focused webview
     contract type modules while preserving the package root and
     `webviewTypes.ts` export surface.
   - Accepted contract: keep `@openaide/app-shell-contracts` package exports
     and existing `./webviewTypes.js` imports compatible. Convert
     `webviewTypes.ts` into a facade over focused modules under
     `src/webview/` for notifications/host requests, preferences/bootstrap
     metadata, diagnostics/settings records, and Webview/App Shell message
     unions. Keep all exported type names, message type strings, union members,
     field names, optionality, and structural shapes unchanged; do not add
     runtime behavior or validation logic in this slice.
   - Implementation status: App Shell Contracts webview type split is
     implemented and ready for integration verification. `webviewTypes.ts`
     remains the compatibility facade, while `src/webview/` owns runtime
     notifications, host request envelopes, preferences, bootstrap, request
     metadata, telemetry, settings records, and Webview/App Shell message
     unions.
   - Review status: `$doomsday-review` correctness, requirements/tests, and
     code-quality passes completed after fixing module ownership and
     unused-import findings; targeted code-quality rerun reported no findings.
   - Verification status: App Shell Contracts webview type split passed
     focused package check/build, repo-wide check, repo-wide test, JSON
     validation, diff whitespace check, exported type-name compatibility diff,
     and changed source-size scan.
   - Proposed next slice: split
     `packages/app-shell-contracts/src/agentCatalog.ts` into focused Agent
     Catalog contract modules while preserving the package root and
     `agentCatalog.ts` export surface.
   - Accepted contract: keep `@openaide/app-shell-contracts` package exports
     and existing `./agentCatalog.js` imports compatible. Convert
     `agentCatalog.ts` into a facade over focused modules under
     `src/agentCatalog/` for icon vocabulary, catalog record types, built-in
     definitions and lookup, custom settings parsing and catalog merging,
     runtime projection, and display fallback. Keep all exported type/value
     names, literal values, field names, optionality, structural shapes,
     built-in records, defaults, custom-agent normalization, runtime projection,
     and display-label fallback behavior unchanged.
   - Implementation status: App Shell Contracts Agent Catalog split is
     implemented and ready for integration verification. `agentCatalog.ts`
     remains the compatibility facade, while `src/agentCatalog/` owns icon
     vocabulary, catalog record types, built-ins, custom settings parsing and
     catalog merging, runtime projection, and display fallback.
   - Review status: `$doomsday-review` correctness, requirements/tests, and
     code-quality passes completed with no findings.
   - Verification status: App Shell Contracts Agent Catalog split passed
     focused package check/build, repo-wide check, repo-wide test, JSON
     validation, diff whitespace check, exported type/value-name compatibility
     diff, changed source-size scan, and value-level catalog smoke check.
   - Proposed next slice: split the Backend Agent runtime contract out of
     `openaide-rs/app-server/src/agent/mod.rs` into a focused Agent module
     while preserving the stable `crate::agent::{...}` import surface.
   - Accepted contract: create `agent/runtime.rs` for runtime-neutral Agent
     session/request structs, `TurnCancellation`, `AgentRuntime`,
     `AgentEventSink`, and `AgentSessionEventSink`. Keep `agent/mod.rs` as the
     module facade and public re-export owner. Preserve every field, derive,
     constructor/helper, trait signature, default implementation, default
     return value, capability-missing error string, and cancellation behavior;
     do not move Agent events, ACP internals, mock behavior, registry behavior,
     prompt content, or tool details in this slice.
   - Implementation status: Backend Agent runtime contract split is
     implemented and ready for integration verification. `agent/runtime.rs`
     owns the runtime-neutral contract, while `agent/mod.rs` remains the
     stable facade and re-export owner.
   - Review status: `$doomsday-review` correctness, requirements/tests, and
     code-quality passes completed with no findings.
   - Verification status: Backend Agent runtime contract split passed focused
     Rust format/check/tests, repo-wide check, repo-wide test, JSON
     validation, diff whitespace check, and changed source-size scan.
   - Accepted next slice: split the VS Code App Shell webview messaging
     dispatcher out of `apps/vscode-extension/src/webview/messaging.ts` while
     preserving `handleWebviewMessage` as the stable facade. The slice creates
     focused modules for generic safe field extraction, MessageContext typing,
     Settings/diagnostics routes, Agent/session routes, shell capability and
     surface routes, and Task/chat routes.
   - Contract: preserve every current message type, route order, runtime call,
     postback shape, error-tagging field, logging redaction behavior, workspace
     fallback, custom Agent save/delete side effect, Task snapshot response,
     and workspace path validation behavior. Do not change App Server Protocol,
     runtime RPC semantics, shared Frontend behavior, or VS Code command
     registration in this slice.
   - Verification target: existing `messaging.test.ts` remains the behavioral
     regression suite; the slice must also pass repo-wide TypeScript checks and
     keep production source files below the project size limit.
   - Implementation status: VS Code webview messaging split is implemented,
     reviewed, verified, and committed. `messaging.ts` remains the stable
     facade; route ownership is split across focused webview messaging modules.
   - Accepted next slice: split the VS Code App Shell host terminal runtime out
     of `apps/vscode-extension/src/runtime/hostTerminal.ts` while preserving
     `registerTerminalHostHandlers` and `TerminalHostManager` as the stable
     public surface.
   - Contract: create focused modules for terminal record/types, request param
     parsing, terminal environment/PATH repair, and output decoding/truncation.
     Preserve every terminal host method name, runtime registration/disposal
     behavior, session ownership check, workspace `cwd` validation, spawn
     options, kill/release/force-kill timing, exit waiter behavior, integrated
     terminal env expansion, Codex bundled PATH repair, output byte limiting,
     UTF-8 decoder handling, and existing test behavior. Do not change runtime
     RPC semantics, workspace boundary policy, terminal lifecycle policy, or
     VS Code command registration in this slice.
   - Verification target: existing `hostTerminal.test.ts` remains the behavior
     regression suite; the slice must also pass repo-wide TypeScript checks and
     keep production source files below the project size limit.
   - Implementation status: VS Code host terminal split is implemented,
     reviewed, verified, and committed. `hostTerminal.ts` remains the stable
     manager/registration facade; param parsing, environment construction,
     output handling, and terminal types are split into focused modules.
   - Accepted next slice: split the VS Code RuntimeClient JSON-RPC internals out
     of `apps/vscode-extension/src/runtime/rpcClient.ts` while preserving
     `RuntimeClient` as the stable public shell runtime client.
   - Contract: create focused modules for JSON-RPC wire types/parsing, runtime
     line classification, shared client listener/handler types, and
     Backend-initiated host request response handling. Keep product convenience
     methods, pending request ownership, runtime process startup, notification
     listener ownership, and host request handler registry in `RuntimeClient`.
     Preserve every runtime method name, request params shape, timeout value,
     startup concurrency behavior, pending rejection on runtime exit/dispose,
     notification dispatch behavior, host request error codes, host response
     sanitization, and closed-stdin logging behavior. Do not change App Server
     Protocol semantics, runtime process ownership, VS Code shell capabilities,
     or task/Agent/settings route behavior in this slice.
   - Verification target: existing `rpcClient.test.ts` remains the behavior
     regression suite; the slice must also pass repo-wide TypeScript checks and
     keep production source files below the project size limit.
   - Implementation status: VS Code RuntimeClient split is implemented,
     reviewed, verified, and committed. `RuntimeClient` remains the stable
     shell runtime client; JSON-RPC wire parsing, runtime line classification,
     host request responses, and shared client types are split into focused
     modules.
   - Accepted next slice: split Backend `protocol_edge` message and response
     construction helpers out of `openaide-rs/app-server/src/protocol_edge.rs`
     while preserving `RpcGateway` as the stable public coordinator.
   - Contract: create private submodules for inbound/outcome/response types and
     protocol response/error construction. Keep initialize gating, method
     dispatch, lifecycle admission, client registration, subscription handling,
     transport-close observation, and snapshot coordination in `RpcGateway`.
     Preserve every public type name re-exported from `protocol_edge`, every
     method route, error code/message/recoverability/target, response envelope
     meta behavior, snapshot cursor behavior, and existing tests. Do not move
     product decisions into the edge helper modules, change App Server Protocol
     records, or alter state/client lifecycle behavior in this slice.
   - Verification target: existing `protocol_edge` tests remain the behavior
     regression suite; the slice must also pass Rust format/check/tests,
     repo-wide checks, and the production source-size scan.
   - Implementation status: Backend protocol_edge split is implemented,
     reviewed, verified, and committed. `RpcGateway` remains the stable public
     coordinator; message/outcome types and response/error construction are
     split into private protocol-edge helper modules.
   - Accepted next slice: split App Server Client state ingestion snapshot
     update helpers out of `packages/app-server-client/src/stateIngestion.ts`
     while preserving `createSubscriptionIngestionState` and
     `applySubscriptionEvent` as the stable public API.
   - Contract: create focused modules for pure subscription snapshot updates,
     Task snapshot updates, and task-navigation scope filtering. Keep scope
     matching, cursor validation, resync classification, ignored/applied result
     construction, and state cursor advancement in `stateIngestion.ts`.
     Preserve every public type/export, every resync reason, project-filtered
     task navigation behavior, snapshot replacement behavior, chat append/chunk
     behavior, pending request upsert behavior, and existing tests. Do not
     introduce untyped protocol payload handling, product state ownership in the
     Frontend client, or protocol semantic changes in this slice.
   - Verification target: existing `stateIngestion.test.ts` remains the
     behavior regression suite; the slice must also pass App Server Client
     check/test, repo-wide checks, and the production source-size scan.
   - Implementation status: App Server Client state ingestion split is
     implemented, reviewed, verified, and committed. `stateIngestion.ts`
     remains the public ingestion API; snapshot replacement, Task updates, and
     task-navigation updates are split into focused helpers.
   - Accepted next slice: split App Server Protocol TypeScript generator helper
     groups out of `openaide-rs/app-server-protocol/src/typescript.rs` while
     preserving `typescript::bindings()` and generated TypeScript output.
   - Contract: keep `typescript.rs` as the generator facade and output-order
     owner. Move method constant emission, `ts_rs` declaration emission, and
     typed method-map/alias emission into focused private modules. Preserve
     every emitted line, generated binding path, public Rust module name, and
     protocol type/method surface.
   - Verification target: `typescript::tests` plus `npm run protocol:generate`
     and `npm run protocol:check` remain the generated-output regression
     checks; the slice must also pass Rust format/check/tests, repo-wide
     checks, and the production source-size scan.
   - Implementation status: App Server Protocol TypeScript generator split is
     implemented, reviewed, verified, and committed. The facade remains the
     stable `bindings()` entry point; constants, declarations, and method maps
     live in focused private generator modules.
   - Accepted next slice: split Task Turn Lifecycle create workflows out of
     `openaide-rs/app-server/src/tasks/turn_lifecycle/create.rs` while
     preserving `TaskTurnLifecycle::create_prompt_start` and
     `TaskTurnLifecycle::create_adopted_session` as the lifecycle-internal
     workflow entry points.
   - Contract: keep `create.rs` as the create-workflow facade and shared
     commit-option/config-option helper owner. Move first-prompt Task creation,
     adopted external session creation, and shared create-title/required-field
     helpers into focused private modules. Preserve validation order, Agent
     session start/load params, config option behavior, TaskRecord fields,
     message order, session guard close/commit behavior, attach/finalize error
     handling, spawned turn behavior, and existing tests. Do not change Task
     mutation semantics, Agent runtime behavior, storage records, protocol
     records, or public service APIs in this slice.
   - Verification target: Task Turn Lifecycle mutation-boundary tests and
     runtime contract Task creation/adoption tests remain the behavior
     regression suites; the slice must also pass Rust format/check/tests,
     repo-wide checks, and the production source-size scan.
   - Implementation status: Task Turn Lifecycle create workflow split is
     implemented, reviewed, verified, and committed. `create.rs` remains the
     create facade; first-prompt creation, adopted-session creation, and shared
     helpers live in focused private modules.
   - Accepted next slice: split VS Code App Shell skill settings scanning out
     of `apps/vscode-extension/src/settings/skills.ts` while preserving
     `scanSkills`, `parseSkillMetadata`, and `SkillScanBase` as the stable
     settings API.
   - Contract: keep `skills.ts` as the scan facade and scan-limit policy owner.
     Move filesystem discovery, SKILL.md metadata parsing, and skill record
     assembly/shadowing into focused settings modules. Preserve workspace and
     global base ordering, nested dot-directory discovery, missing/unreadable
     root handling, warning sanitization, scan limit behavior, metadata parsing
     fallback, shadowing priority, public exports, and existing tests. Do not
     change settings snapshot contracts, runtime RPC behavior, App Server
     Protocol records, or product UX text in this slice.
   - Verification target: existing `skills.test.ts` remains the behavior
     regression suite; the slice must also pass VS Code Extension tests,
     repo-wide checks, and the production source-size scan.
   - Implementation status: VS Code skill settings scan split is implemented,
     reviewed, verified, and committed. `skills.ts` remains the public scan
     facade; discovery, metadata parsing, record assembly, and helper types
     live in focused settings modules. The slice also fixed a narrow
     `messagingTasks.ts` type-narrowing issue exposed by the extension check.
   - Accepted next slice: split ACP trace state/session/file helpers out of
     `openaide-rs/app-server/src/agent/acp_trace.rs` while preserving
     `AcpTraceState`, `AcpTraceSession`, `AcpTraceStatus`,
     `RuntimeSettings`, and `RuntimeDeveloperSettings` as the stable Agent
     trace API.
   - Contract: keep `acp_trace.rs` as the public Agent trace facade and
     re-export owner. Move mutable trace enablement/status state, per-session
     record/line handling, trace-file creation/writing, and filename/env helper
     parsing into focused private modules. Move inline tests to
     `acp_trace/tests.rs`. Preserve environment variable names, default
     diagnostics directory, accepted enable values, runtime settings shape,
     trace JSONL fields, trace-opened event, sensitive marker, file naming
     sanitization/truncation, failure logging, eprintln diagnostics, lazy file
     creation, and existing tests. Do not change ACP protocol behavior, Agent
     runtime behavior, logging sanitization policy, or runtime settings
     protocol shape in this slice.
   - Verification target: ACP trace unit tests, ACP session connection trace
     tests, and runtime settings contract tests remain the behavior regression
     suites; the slice must also pass Rust format/check/tests, repo-wide
     checks, and the production source-size scan.
   - Implementation status: Backend ACP trace split is implemented, reviewed,
     verified, and committed. `acp_trace.rs` remains the public Agent trace
     facade; trace state, session recording, file writing, naming/env helpers,
     and tests live in focused private modules.
   - Accepted next slice: split Task transition workflow groups out of
     `openaide-rs/app-server/src/tasks/transitions.rs` while preserving
     `TaskTransitions` as the stable lifecycle transition facade.
   - Contract: keep `transitions.rs` as the facade and constructor owner. Move
     active-turn lookup/cancel/finish transitions, volatile recovery/session
     binding cleanup, create/adopt failure finalization, and shared commit or
     interruption helpers into focused private modules. Preserve every public
     method name/signature, lock timing, commit options, Task status/unread/
     timestamp mutations, interruption reason/message/recoverability, pending
     permission cancellation, running activity completion/error status,
     archived-task recovery coverage, `TaskNotFound` handling for adopted attach
     finalization, active turn matching behavior, and existing tests. Do not
     change Task mutation semantics, TurnRunner behavior, storage records,
     runtime recovery policy, or protocol shapes in this slice.
   - Verification target: Task mutation boundary tests, turn lifecycle/turns
     tests, runtime shutdown/recovery contract tests, and task creation failure
     contract tests remain the behavior regression suites; the slice must also
     pass Rust format/check/tests, repo-wide checks, and the production
     source-size scan.
   - Implementation status: Backend Task transitions split is implemented,
     reviewed, verified, and committed. `transitions.rs` remains the facade;
     active-turn transitions, recovery/session cleanup, failure finalization,
     and shared transition helpers live in focused private modules.
   - Accepted next slice: split Agent prompt-content construction out of
     `openaide-rs/app-server/src/agent/prompt_content.rs` while preserving
     `PromptContentCapabilities`, `PromptContentPolicy`, `PromptContentError`,
     `build_prompt_content_with_policy`, and `validate_prompt_attachments` as
     the stable Agent prompt-content API.
   - Contract: keep `prompt_content.rs` as the public facade and policy/error
     owner. Move attachment routing, payload-to-ACP-content conversion,
     resource-link construction, payload field parsing, media-kind detection,
     and attachment error formatting into focused private modules. Preserve
     text block ordering, attachment order, fallback from unsupported embedded
     file payloads to resource links, image/audio capability checks, default
     image/audio MIME types, embedded-context capability checks, text/blob
     embedded resource construction, synthetic embedded URIs, resource-link
     MIME propagation, payload field aliases, error message text, public test
     helper behavior, and existing tests. Do not change ACP schema mapping,
     prompt capability policy, attachment URI rules, Agent runtime behavior,
     Task attachment semantics, or protocol/storage records in this slice.
   - Verification target: prompt-content unit tests, ACP prompt-content tests,
     prompt attachment runtime contract tests, Rust format/check/tests,
     repo-wide checks, and the production source-size scan.
   - Implementation status: Backend prompt-content split is implemented,
     reviewed, verified, and committed. `prompt_content.rs` remains the public
     Agent prompt-content facade; attachment routing, payload parsing, content
     block conversion, and resource helpers live in focused private modules.
   - Accepted next slice: split App Server Protocol Task snapshot records out
     of `openaide-rs/app-server-protocol/src/snapshot/task.rs` while
     preserving `openaide_app_server_protocol::snapshot::*` as the stable
     public namespace.
   - Contract: keep `snapshot/task.rs` as the Task render-model facade and
     re-export owner. Move preparation/setup records, live Agent config and
     slash-command records, and send-capability records into focused private
     modules under `snapshot/task/`. Preserve every Rust type name, serde
     shape, TypeScript declaration name/order, public re-export, and generated
     protocol output. Do not change App Server snapshot semantics, method or
     event records, runtime behavior, storage behavior, or Frontend state
     ingestion in this slice.
   - Verification target: App Server Protocol format/check/tests, generated
     protocol output checks, repo-wide TypeScript checks/tests, and the
     production source-size scan.
   - Implementation status: App Server Protocol Task snapshot split is
     implemented, reviewed, verified, and committed. `snapshot/task.rs` remains
     the public Task render-model facade; preparation/setup, live Agent data,
     and send-capability records live in focused private modules.
   - Accepted next slice: split ACP active-session registry operations out of
     `openaide-rs/app-server/src/agent/acp_active_session_manager.rs` while
     preserving `AcpActiveSessionManager` as the stable active Agent session
     runtime facade.
   - Contract: move session-map lookup, insert, removal, duplicate-id handling,
     cancellation/close/delete dispatch, event-sink attachment lookup, and
     shutdown close-task extraction into a focused private registry module.
     Keep ACP worker startup, registry config lookup, trace/auth state, startup
     timeout handling, terminal error recording, and runtime thread spawning in
     `AcpActiveSessionManager`. Preserve every public method signature,
     duplicate active-session error text, not-ready error text, close-on-duplicate
     behavior, idempotent cancel/close behavior, delete requiring active session,
     resume capability behavior, shutdown close-task behavior, and existing
     tests. Do not change Agent runtime behavior, ACP worker protocol, storage,
     protocol shapes, or App Server lifecycle in this slice.
   - Verification target: ACP active-session runtime tests, Rust
     format/check/tests, repo-wide checks, and the production source-size scan.
   - Implementation status: Backend ACP active-session registry split is
     implemented, reviewed, verified, and committed. `AcpActiveSessionManager`
     remains the active session runtime facade; active-session map operations,
     lookup errors, close/delete/cancel dispatch, and shutdown close-task
     extraction live in a focused private registry module. Review found and the
     slice fixed missing inactive-session edge-case test coverage.
   - Accepted next slice: split ACP probe/authenticate blocking runner logic
     out of `openaide-rs/app-server/src/agent/acp_runtime_kernel.rs` while
     preserving `AcpRuntimeKernel` as the Agent runtime coordinator.
   - Contract: move the synchronous thread spawning, mpsc response channels,
     timeout margin handling, ACP config lookup for probe/authenticate, host
     bridge cloning, and auth-method cache recording into a focused private
     probe/auth runner module. Keep option-session routing, active-session
     routing, shutdown close aggregation, public timeout constants, and the
     runtime coordinator facade in `AcpRuntimeKernel`. Preserve default timeout
     values, test-only `probe_with_timeout`, empty auth-method validation,
     probe/auth timeout error text, missing command error behavior, auth-method
     cache update behavior, and existing tests. Do not change ACP protocol
     flow, host capability handling, option sessions, active sessions, storage,
     protocol shapes, or App Server lifecycle in this slice.
   - Verification target: ACP probe/auth tests, runtime contract tests, Rust
     format/check/tests, repo-wide checks, and the production source-size scan.
   - Implementation status: Backend ACP probe/auth runner split is implemented,
     reviewed, verified, and committed. `AcpRuntimeKernel` remains the runtime
     coordinator; blocking probe/auth config lookup, host bridge cloning,
     thread/channel timeout handling, auth validation, and auth-method cache
     recording live in a focused private runner module. Review found and the
     slice fixed missing runner-specific auth/probe edge-case test coverage.
   - Accepted next slice: split ACP session close/delete termination helpers out
     of `openaide-rs/app-server/src/agent/acp_session_lifecycle.rs` while
     preserving session lifecycle startup, load, list, and projection helpers
     in the lifecycle module.
   - Contract: move `close_active_session` and `delete_active_session` into a
     focused private termination module. Preserve trace direction and event
     names, support-capability checks, close no-op when unsupported, delete
     `CapabilityMissing` text when unsupported, request construction, response
     trace recording, error trace recording, ACP error mapping, `AcpSessionRunner`
     public behavior, and existing tests. Do not change active-session startup,
     load/resume replay, list-sessions filtering, prompt
     running, Agent runtime behavior, storage, protocol shapes, or App Server
     lifecycle in this slice.
   - Verification target: ACP active-session runtime tests, ACP session tests,
     Rust format/check/tests, repo-wide checks, and the production source-size
     scan.
   - Implementation status: Backend ACP session termination split is already
     implemented, verified, and committed. `acp_session_termination.rs` owns
     close/delete request construction, tracing, capability checks, response
     handling, and ACP error mapping, while `acp_session_lifecycle.rs` keeps
     active-session startup, load/replay, list-session filtering, and session
     projection helpers.

7. **Storage And Lifecycle**
   - Finalize state root, shared App Server discovery, storage concurrency, client attachment lifetime, graceful shutdown, crash recovery, and live task ownership.
   - Protect storage from concurrent writers even when several shells start near the same time.
   - Current progress: Storage state roots are protected by a per-state-root
     writer lock and clean/unclean runtime marker. App Server client
     attach-or-launch uses endpoint records plus launch locks. `RpcGateway`
     now exposes reconnect-grace client expiry, interrupts expired
     client-scoped server requests, preserves reattached clients against stale
     expiry attempts, and transitions lifecycle to draining when the last
     initialized client expires. LocalHttp clients now use typed
     `client/heartbeat`; normal product requests also refresh liveness; a
     runtime expiry loop removes inactive clients after reconnect grace and
     drives the same lifecycle/server-request expiry path. Remaining concrete
     lifecycle work is a final audit for stale legacy cleanup and missing
     integration checks. Shutdown/clean-release wiring is implemented:
     TaskProductApi shuts down live task/Agent runtime before marking storage
     clean, LocalHttp liveness shutdown removes the current endpoint record only
     after clean shutdown succeeds, and failed shutdown leaves the unclean
     marker path intact. Final cleanup audit removed stale `legacy*` helper
     names from typed App Server-to-Frontend mapping code; remaining use of
     "legacy" in this plan refers to historical migration notes or explicitly
     named compatibility boundaries.

8. **Cleanup And Verification**
   - Delete superseded legacy code.
   - Update scripts and documentation.
   - Add integration tests at protocol and runtime boundaries before relying on new behavior.
   - Current progress: Frontend active-task snapshot polling is deleted. Active
     task UI updates now rely on App Server events or shell runtime
     notifications instead of a 600ms UI refresh loop. Typed startup task-list
     failures now surface as navigation UI errors instead of falling back to a
     legacy `task.list` bridge request. Shared Frontend follow-up sends no
     longer emit legacy `session.prompt`; typed `task/send` is required, and
     non-App-Server attachment rows produce a visible composer error. Shared
     Frontend permission answers no longer emit legacy `permission.respond`;
     only App Server-backed permission request ids are answerable. App Server
     project snapshots are now stored in shared Frontend state, and new-task
     selection carries `projectId`. Normal new-task submit now uses typed
     `task/create`, typed `task/setConfigOption`, and typed `task/send`.
     Native session adoption now uses typed `task/adoptNativeSession`; shared
     Frontend no longer emits legacy `task.create`, and unavailable App Server
     adoption renders a local error. Shared Frontend task cancel no longer emits
     legacy `task.cancel`; typed `task/cancel` is required and failures render
    as task composer errors. Startup and task-open paths no longer fall back to
    legacy `task.list`, `task.markRead`, or `task.snapshot` bridge reads; typed
    initialize/read failures surface as renderable Frontend errors.
    App Server-owned Agent delete and enable/disable
     settings actions no longer fall back to legacy shell mutations; when the
     typed App Server request path is unavailable, Settings renders a local
     error instead. Archive mode now has typed App Server coverage:
     `task/list` carries an explicit archive filter, `task/setArchived` moves
     Tasks between active and archived navigation lists, and shared Frontend
     archive/restore callbacks use typed requests whenever an App Server
     connection is available. Navigation native-session listing now has typed
     App Server coverage through `agent/listSessions`; App Server resolves
     `projectId` to the local Agent cwd internally, while typed responses expose
     only safe project labels and session metadata. Configuration Options come
     exclusively from each Task's real Native Session and changes use
     `task/setConfigOption`; no pre-Task option session or Agent-level option API exists.
     Settings startup uses typed `settings/getAgentDetails` after App Server
     initialize and skips legacy `settings.snapshot`.
     Manual Settings refresh is typed-only and reports an App Server-required
     error instead of falling back to legacy `settings.snapshot`.
     Shared Frontend clients no longer ingest legacy product bridge results or
     errors in the host message router, so stale `task.*`, `agent.*`, and
     `settings.snapshot` messages cannot replace typed App Server state;
     shell-local messages such as workspace roots and developer unlock still route.
     Shared navigation archive/listing callbacks and new-task option edits now
     report App Server connection errors instead of emitting legacy
     `task.archive`, `task.restore`, `task.list`, or
     `session.setConfigOption` product bridge requests.
     Shared startup and task-open paths now also report App Server connection
     errors instead of emitting legacy `task.list`, `task.markRead`,
     `task.snapshot`, or `settings.snapshot` product bridge requests.
    Shared native-session listing now reports an App Server connection error
    instead of emitting legacy `agent.listSessions` bridge requests. Agent
    authentication now uses typed `agent/authenticate`, refreshes typed Settings
    details after success, and reports a visible Settings error without App
    Server instead of emitting legacy `agent.authenticate`. Legacy host result
    handlers no longer issue follow-up product requests for native-session
    pagination, post-auth Settings refresh, or task-refresh snapshots.
    Shared host message routing no longer ingests legacy task, Agent, or
    settings product results/errors at all; it keeps only shell-local routing
    such as workspace roots, developer unlock, surface navigation, and context
    files. Chat paging and tool detail reads no longer use shell-host messages;
    they use typed `task/chatPage` and `task/toolDetail` App Server requests.
    Standalone dev-host routing no longer emulates legacy product requests, and
    no-backend Agent option autoload reports an App Server connection error
    instead of posting `agent.configOptions`. VS Code webview bootstrap no
    longer injects shell-collected Agent lists; Frontend gets Agents from App
    Server initialize.
    Legacy stdio dispatch no longer accepts Agent authentication,
    native-session listing, or Agent config-option product methods; typed
    `agent/*` App Server Protocol coverage owns those paths now.
    The Rust runtime binary now defaults to App Server Protocol stdio; VS Code
    host-side typed App Server requests use the same LocalHttp handoff boundary
    as webviews. VS Code opts into `shell-control-stdio` explicitly only for remaining
    shell-local runtime requests, while App Server webview handoff continues to
    use `app-server-handoff`.

## Module Grill Queue

After the top-level module interface is accepted, grill these module interfaces one by one:

1. Repository shape and package naming.
2. Backend/Frontend protocol seam.
3. App Server `server_requests` and Backend-initiated Frontend/App Shell request lifecycle.
4. Backend process lifecycle, shared-instance discovery, and state roots.
5. Backend storage model and concurrent access protection.
6. Backend Rust module split beyond the first skeleton.
7. Agent runtime and ACP integration.
8. Project, Task, Native Session, and history model.
9. Agent settings, identity, status, and cleanup.
10. Frontend shared architecture and shell injection API.
11. Web App shell.
12. Desktop App shell.
13. VS Code Extension shell.
14. Build, dev, and test workflow.

## Acceptance Rules

- Do not implement broad refactors until the relevant top-level or module decision is recorded.
- Keep accepted decisions in this plan while they are planning-level; promote stable architecture decisions to ADRs when they become rules.
- Ask one grill question at a time.
- Avoid drilling into module internals or detail-level choices while grilling the top-level Backend/Frontend API; choose reasonable defaults unless the decision changes the seam.
- Every refactor-planning response must end by stating the next planned API/design step or by asking whether to stop planning and implement.
- If a decision needs research, inspect the codebase or relevant docs before asking the user.

## Current Next Step

The initial A0-A9 architecture slices and their completion audit are complete.
Historical "Proposed next slice", "Accepted next slice", and "ready for review"
notes below are retained as implementation history unless a fresh audit names a
concrete current gap.

## Completed Slice Contract: A4 VS Code Reveal File Resolve/Open Path

- App Server Protocol exposes typed `shell/resolveFileReveal` only to an
  initialized VS Code host client advertising the dedicated resolver
  capability. The handle remains bound to the Frontend client that originated
  the live attachment action.
- VS Code `shell/revealFile` handling resolves `fileHandleId` through its
  separate LocalHttp App Server client, validates the resolved path against
  workspace boundaries, opens the document, and responds with
  `{ revealed: true }`. Raw paths never traverse shared Frontend or webview
  messages.
- Missing or rejected handles respond with `{ revealed: false }` without
  falling back to raw path params.
- Reveal handles are unpredictable, client-owned, single-use, and bounded in
  App Server memory. The legacy shell-control resolver is not exposed.
- Tests prove only a capable native VS Code host can resolve an originating
  client's handle once and VS Code opens the result while keeping shared
  request params opaque.

## Completed Slice Contract: A4 Backend Reveal File Handles

- App Server owns an in-memory `ShellFileRevealRegistry` that maps opaque
  reveal handle ids to local file targets.
- Reveal handle ids are generated by App Server and do not encode raw local
  paths.
- The registry stores raw paths only in Backend-owned memory and returns safe
  labels for protocol display.
- `ServerRequestRuntime` can open typed waitable `shell/revealFile` requests
  with `fileHandleId` and safe label only.
- Tests prove reveal handle ids are opaque, relative paths are rejected, and
  typed reveal request params do not include raw paths.

## Completed Slice Contract: A4 Task Update Server-Request Delivery Hook

- `RpcGateway` exposes one connection-scoped operation that publishes a Task
  update and drains server-request envelopes newly deliverable to that same
  initialized connection.
- The stdio protocol edge uses that operation for `task.updated` runtime
  notifications instead of composing event publication and request draining at
  the transport edge.
- The hook covers task-scoped requests opened after a Task subscription already
  exists, including typed `secret/read` requests created by Task preparation.
- Tests prove a task-scoped `secret/read` opened directly in
  `ServerRequestRuntime` after subscription is delivered by the Task update
  hook.
- Local HTTP still has no push notification channel; it can deliver server
  requests on protocol responses, while runtime notification push remains a
  separate transport design topic.

## Completed Slice Contract: A4 Active ACP Secret Resolver Injection

- `AgentSessionStart` and `AgentSessionLoad` can carry an optional
  `AgentSecretResolver`; active ACP session startup passes it into
  `AcpAgentConfig` when constructing the ACP process.
- ACP probe, auth, and options-session startup keep the legacy host secret
  bridge because they are not Task-scoped visible surfaces yet.
- Responsive Task preparation injects an Agent secret resolver that resolves
  each configured secret env name through typed task-scoped `secret/read`.
- The typed resolver uses the existing VS Code custom-Agent secret key format,
  `openaide.agent.{agentId}.env.{name}`, to avoid orphaning configured secrets.
- Tests prove injected secret env values reach the ACP process environment.

## Completed Slice Contract: A4 Task Preparation Secret Request Publication

- `TaskProductApi` owns a narrow preparation-time secret request helper that
  opens a typed task-scoped `secret/read` request, publishes the current Task so
  subscribed Frontend clients can render the pending request, then waits for the
  shell response.
- Task publication for non-mutating live request changes goes through the
  shared Task mutation module instead of direct notifier access from product
  workflows.
- The server-request runtime remains the source of truth for pending request
  lifecycle and response waiting; Task storage is not mutated just to surface a
  live pending request.
- Tests prove the preparation helper publishes `task.updated`, the task
  subscription receives the typed `secret/read` envelope, and the helper reads
  the accepted shell response.

## Completed Slice Contract: A4 Waitable Shell/Secret Producer Primitive

- `ServerRequestRuntime` can open waitable client-scoped server requests and
  return the concrete deliveries that protocol-edge code must send to the
  selected shell client.
- `ServerRequestRuntime` can also open waitable task-scoped server requests
  before a responder is available; normal subscription/responder lifecycle then
  delivers the request when a client subscribes to the Task.
- Accepted client responses are stored separately from permission waiters and
  can be waited on by request id.
- Timeout interrupts the pending request and removes wait state instead of
  leaving durable-looking pending UI behind.
- Typed helpers exist for `secret/read` and `shell/showNotification`, so
  Backend callers do not construct method strings or JSON payloads by hand.
- This slice intentionally does not move ACP `secret_env` yet: current Agent
  startup can block before protocol traffic has a chance to deliver the
  Backend-initiated request, so the next slice must add an async delivery path
  through task preparation/session startup.
- Tests prove accepted waitable responses, delivered typed request envelopes,
  deferred task-scope delivery, and timeout cleanup.

## Completed Slice Contract: A4 Shell/Secret Server Request Bridge

- Frontend subscribes to `BackendConnection.serverRequests` and forwards only
  shell-owned request categories to the App Shell: `secret/read`,
  `shell/showNotification`, and `shell/revealFile`.
- Permission requests stay in the Task UI flow and are not auto-answered by the
  shell bridge.
- App Shell request/result messages are explicit shell contracts, separate from
  normal App Server transport request/response messages.
- VS Code answers `secret/read` from `SecretStorage` by key and
  `shell/showNotification` through native notification UI.
- VS Code answers `shell/revealFile` with `{ revealed: false }` until Backend
  owns a real opaque file-handle resolver; it does not fall back to raw paths.
- Legacy webview BackendConnection bridge exposes the same `serverRequests`
  shape as LocalHttp so Frontend lifecycle code has one interface.
- Tests prove Frontend forwarding/response handling, permission exclusion,
  VS Code secret reads, VS Code notification action mapping, and safe reveal
  fallback.

## Completed Slice Contract: A4 Server Request Category Typing

- The App Server Protocol source defines typed Backend-initiated request
  methods for `permission/request`, `secret/read`, `shell/showNotification`,
  and `shell/revealFile`.
- Generated TypeScript bindings expose typed constants, params, responses,
  method maps, and response result maps for all four request methods.
- The generic `BackendConnection.respond(requestId, result)` channel remains
  the external response seam; generated method maps type the allowed response
  shapes.
- `shell/revealFile` uses an opaque App Server file handle id plus optional
  safe label, not a raw filesystem path.
- Tests prove safe camelCase request shapes, generated method-map coverage,
  generated binding freshness, and TypeScript consumer compatibility.

## Completed Slice Contract: A7 VS Code LocalHttp Webview Bootstrap

- VS Code navigation and editor webview surfaces render an immediate preparing
  shell while App Server handoff runs.
- On handoff success, VS Code renders the normal Frontend with ephemeral
  LocalHttp connection info in bootstrap.
- On handoff failure, VS Code logs the failure and renders the normal Frontend
  without LocalHttp connection info. Product requests report App
  Server-required errors rather than falling back to legacy product bridge
  traffic.
- New Task panel adoption preserves an already acquired LocalHttp connection
  when the panel is re-rendered with the created Task id.
- Pending handoff renders are generation-guarded so disposed or superseded
  webviews are not mutated after async completion.
- Final webview bootstrap is built after handoff completes, so current
  preferences and Agent catalog state are used instead of stale preparing-time
  values.
- Tests prove pending rendering, successful LocalHttp bootstrap injection, and
  no-connection bootstrap on handoff failure, disposed pending panels, and
  New Task adoption preserving LocalHttp bootstrap.

## Completed Slice Contract: A7 Runtime LocalHttp Handoff Surface

- `OPENAIDE_RUNTIME_PROTOCOL=app-server-handoff` uses the shared Rust
  `AttachOrLaunchHandoff`.
- If a compatible LocalHttp endpoint already exists, handoff prints
  `{ kind: "localHttp", endpointUrl, authToken }` and exits without starting a
  second App Server.
- If this process wins launch election, handoff starts the App Server Protocol
  edge, publishes the LocalHttp endpoint, prints the same connection JSON, and
  keeps serving.
- The connection JSON is the handoff process's only stdout message. After that
  line, product traffic and Task event delivery use LocalHttp; stdin remains
  only as a parent-lifetime signal and is not a second App Server transport.
- LocalHttp handoff advertises ACP filesystem and terminal client capabilities
  as unavailable because it has no typed responder route for those Agent host
  requests. Plain protocol stdio retains its serviced host-request channel.
  `AcpHostRequestTransport` is the migration seam for adding a truthful typed
  LocalHttp responder later; support must not be advertised before that route
  exists.
- The launch lock is held by the elected launcher process while it starts the
  protocol edge and remains alive.
- VS Code `RuntimeProcess.startAppServerConnection()` spawns handoff mode and
  validates the returned LocalHttp connection info instead of reading endpoint
  files or duplicating endpoint discovery in TypeScript.
- VS Code accepts only loopback `http` LocalHttp handoff endpoints with a
  `/probe` path, explicit port, and non-empty token.
- VS Code bounds handoff stdout reads with a timeout and maximum line size so a
  broken child cannot freeze App Server connection setup indefinitely.
- If the launched handoff child exits, VS Code clears its cached connection so
  the next request performs a fresh handoff.
- Shared Frontend accepts an App Shell-owned `AppServerSession`. The VS Code App
  Shell supplies a typed webview bridge to the extension host's single reliable
  LocalHttp session; browser App Shells retain direct browser-safe transports.
- VS Code webviews no longer receive LocalHttp endpoint URLs or process tokens,
  and their CSP no longer allows direct App Server connections.
- Shell-control stdio `runtime.health` advertises only `runtime.health` and
  `runtime.shutdown`; product and raw-path resolver methods are no longer
  presented through the legacy transport.
- Runtime contract tests prove handoff launch serves real LocalHttp
  `client/initialize` and a second handoff reuses the existing endpoint.

## Completed Slice Contract: VS Code Single App Server Client

- One VS Code extension host/window owns one stable `clientInstanceId`, one
  initialized reliable LocalHttp session, and one physical receive loop.
- Navigation, Task, New Task, and Settings webviews are render views behind the
  host session. Their typed adapters preserve requests, state subscriptions,
  recovery/status events, and Backend-initiated requests without exposing
  transport credentials.
- View attachment ids are extension-local routing identities, not App Server
  client identities. Closing a view releases only that view's observers; the
  host session closes with the extension runtime.
- Equal subscription scopes share the host session's existing replica and
  underlying App Server subscription.
- Native VS Code Task and Settings panels do not load Task Navigation or Agent
  native-session history because they do not render the sidebar. The Navigation
  view remains the owner of that work; the Web workbench retains it on routes
  where its integrated sidebar is rendered.
- Conformance tests prove multiple webviews initialize one client and that VS
  Code webview HTML contains no endpoint or token material.

## Superseded Slice Contract: A7 Shell Bootstrap LocalHttp Connection Contract

The following records the earlier direct-webview transport and is superseded for
VS Code by the single-client contract above. Browser App Shells still use the
direct bootstrap pattern where applicable.

- App Shell bootstrap records can carry optional ephemeral LocalHttp App Server
  connection info: endpoint URL and process token.
- VS Code webview HTML serializes that bootstrap field only when supplied and
  adds a CSP `connect-src` for the LocalHttp endpoint origin.
- Shared Frontend parses the bootstrap field defensively and chooses
  `createLocalHttpBackendConnection` when present.
- Shared Frontend uses its existing `sessionStorage` client instance id as the
  LocalHttp connection id, preserving tab-local shell-instance identity.
- When no LocalHttp connection info is present, the webview still has
  shell-local host messages, but product traffic does not fall back to the
  legacy webview bridge.
- This slice does not discover endpoint records, launch App Server, or decide
  shell process policy in the browser Frontend.
- Tests prove webview CSP/bootstrap serialization and Frontend LocalHttp
  connection selection with the expected bearer token and connection id.

## Completed Slice Contract: A7 LocalHttp BackendConnection Adapter

- `@openaide/app-server-client` exposes `createLocalHttpBackendConnection` for
  shell-provided ephemeral LocalHttp endpoint info.
- The adapter sends typed App Server Protocol JSON-RPC requests with
  `Authorization: Bearer <process-token>` and `X-OpenAIDE-Connection-Id`.
- It gates product requests behind `client/initialize`, unwraps typed response
  envelopes, routes App Server events to the event channel, and routes
  Backend-initiated server requests to an explicit `serverRequests` channel.
- `BackendConnection` now models Backend-initiated requests separately from
  state events and client responses.
- `respond()` sends JSON-RPC client responses over the same LocalHttp transport.
- App Server LocalHttp and stdio share the same client-response parser, and
  client responses require JSON-RPC 2.0 plus exactly one of `result` or `error`.
- This slice still does not discover endpoint records, launch App Server, or
  embed shell-specific bootstrap policy in the browser Frontend.
- Tests prove LocalHttp request headers, initialize gating, response envelope
  unwrapping, event delivery, Backend-initiated request delivery, protocol
  error propagation, response sending, and malformed client-response rejection.

## Completed Slice Contract: A7 LocalHttp Product Request Transport

- The published loopback LocalHttp endpoint serves both compatibility
  `client/probe` requests and product App Server Protocol requests.
- Product requests require `Authorization: Bearer <process-token>` and
  `X-OpenAIDE-Connection-Id`; the connection id becomes the stable transport
  connection id inside `client_hub`.
- Product responses are ordered JSON-RPC wire message arrays so a single HTTP
  response can carry the method response plus events and Backend-initiated
  requests for that connection.
- Compatibility `client/probe` requests without a connection id keep the
  previous object response shape used by endpoint probing.
- Browser-safe preflight is handled locally with CORS headers for
  Authorization, Content-Type, and X-OpenAIDE-Connection-Id.
- Accepted LocalHttp connections are handled by per-connection workers so a
  slow product request cannot block probe traffic or other clients.
- Tests prove protocol connection-id dispatch, notification rejection,
  preflight behavior, published endpoint product request behavior, and
  continued availability while one connection is stalled.

## Completed Slice Contract: A7 Launch Handoff Loop

- Shared attach-or-launch handoff lives in the App Server client layer, not in a
  shell.
- It reuses `AttachOrLaunchRunner::run_with_local_transports` and therefore
  keeps endpoint discovery, stale cleanup, and LocalHttp `client/probe`
  classification in one place.
- Compatible endpoint discovery returns an attach target without waiting.
- Missing endpoint discovery returns a launch lock handoff for the elected
  launcher.
- Busy launch lock waits through an injected waiter and retries endpoint
  discovery.
- Waiting is bounded by policy and returns a typed still-in-progress error
  instead of spinning forever.
- The default waiter is simple sleep-based behavior for shell/bootstrap use, but
  tests can inject deterministic waiters.
- Tests prove launch election, immediate attach, wait-then-attach through a real
  published LocalHttp probe endpoint, and bounded wait failure.

## Completed Slice Contract: A7 LocalHttp Probe Endpoint Integration

- The listener is probe-only and handles one HTTP request per accepted
  connection.
- It supports POST requests with `Content-Length` only; missing or malformed
  HTTP framing returns a local HTTP error without calling protocol code.
- It extracts the Authorization header and request body, then delegates to the
  existing `LocalHttpProbeHandler`.
- It writes HTTP status, `Content-Length`, and JSON content type when a body is
  present.
- Socket read/write timeout or malformed HTTP framing must not expose probe
  facts or auth tokens.
- Protocol-edge App Server startup publishes a runtime endpoint record for the
  loopback LocalHttp probe endpoint, using a process-scoped high-entropy auth
  token stored only in the protected runtime endpoint record.
- Endpoint record identity and `client/probe` identity are derived from the same
  shared App Server probe facts.
- Publishing the reusable endpoint is fail-closed: an App Server process must
  not keep the storage writer open while undiscoverable to other shells.
- Endpoint cleanup is guarded by ownership facts and removes only the current
  process record during normal shutdown/drop.
- `AttachOrLaunchRunner::run_with_local_transports` uses the concrete LocalHttp
  `client/probe` exchange.
- An integration test must prove a published LocalHttp endpoint record can be
  reused by attach-or-launch through the real protocol probe path.
- This slice does not support normal product traffic beyond `client/probe`.
