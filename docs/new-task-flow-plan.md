# New Task Flow Implementation Plan

Status: accepted design, not implemented

This focused plan is the implementation handoff for the New Task refactor. It resolves `AP-003` and `AP-005` through `AP-008` in `docs/architecture-problems.md` and must be implemented through a feature branch and pull request. A new agent session should read `CONTEXT.md`, `PRODUCT.md`, ADR 0022, `docs/refactor-plan.md`, and this file before changing code.

## Simplification Rule

The primary purpose of this audit and refactor is to reduce bug surface through simpler ownership, state, interfaces, and failure behavior. Implementation must not preserve current complexity merely because it already exists.

- Every retained mechanism must protect a named product invariant or a demonstrated common failure.
- Prefer one owner, one state representation, one ordering mechanism, and one validation pass.
- Prefer visible failure plus explicit recovery over transparent automatic retry.
- Do not add speculative concurrency, replay, persistence, compatibility, or recovery machinery without discussing and recording the concrete scenario first.
- Do not measure simplicity only by line count; remove caller knowledge, invalid state combinations, duplicated policy, hidden ordering, and cross-module coordination.
- Keep only the essential Send invariants: one accepted user action creates one message, an acknowledged message survives restart, and content cannot be delivered to the wrong client, Task, or Native Session.
- If implementation discovers a requirement that conflicts with this plan, stop and discuss it instead of silently expanding the design.

## Replacement Rule

Every agreed flow in this document replaces its corresponding current implementation completely. Remove superseded state, branches, guards, retries, reconciliation, polling, synthetic Chat rows, compatibility adapters, duplicate APIs, legacy protocol shapes, and old persisted development-data formats instead of running old and new architectures in parallel. Do not provide compatibility with the current OpenAIDE implementation and do not add migrations or fallback deserialization for it. If implementation finds a current behavior that an agreed flow does not cover, stop and discuss it rather than retaining it speculatively.

## Outcome

OpenAIDE keeps at most one client-private New Task for each stable `clientInstanceId`. The New Task has a real Task id and acquires a real Agent Native Session, but remains invisible outside its owning client until App Server durably accepts its first user message. Navigation never discards it. While first Send is unresolved, invoking New Task reopens the same submitting instance. The first accepted send promotes the same Task identity into normal visible Task state; a later New Task action creates the next instance.

## Domain Rules

- **New Task** is the canonical term. Do not introduce `Draft Task`, `Established Task`, or `slot` in product language or new implementation interfaces.
- A New Task is private to one `clientInstanceId`.
- One client owns at most one New Task at a time.
- Project Context and Agent are selected before New Task creation and become immutable when its Native Session is created.
- A New Task and a Task with first Send in flight are excluded from Task Navigation, active and archived Task lists, normal history/session discovery, search, and other clients' snapshots and events.
- Ordinary navigation, view unmount, and switching to Settings or an existing Task retain the New Task and its Native Session.
- Only explicit discard removes a New Task before first send.
- The first durably accepted user message atomically makes the same Task visible through normal Task queries and events.
- App Server owns Task identity, Native Session state, options, commands, readiness, capabilities, attachment resolver resources, and first-send promotion.
- Frontend owns unsent text and live composer presentation state.

## Client Identity

Frontend supplies `clientInstanceId` only through `client/initialize`. Transport assigns a connection-local `connectionId`; `ClientHub` maps that connection to the initialized client. Product handlers obtain client identity from this trusted connection context instead of accepting a client id in product request params.

- Browser tabs retain `clientInstanceId` in session-scoped storage with memory fallback.
- VS Code and other native shells issue a stable identity for the shell client/webview lifecycle.
- Reconnect sends the same `clientInstanceId` through `client/initialize`.
- A client that loses its stable identity is a new client and must not recover another client's New Task.

## Default Project And Agent

There are two distinct selection owners:

1. A live client remembers its own last selected Project and Agent locally.
2. App Server persists state-root-wide last-used Project and Agent defaults only as the initial selection for a client that has no retained selection.

Selection priority is:

1. the client's retained valid selection;
2. an explicit App Shell Project hint, such as the current VS Code Project Context;
3. App Server's persisted last-used default when it remains available;
4. a documented deterministic available fallback.

App Server returns the global defaults in the initialized client snapshot. Frontend adopts them only when it has no retained choice and validates them against the current Project and Agent collections. App Server updates the global last-used values as part of the first successful send for the corresponding New Task; no extra preference request is required. Updates do not overwrite selections already held by connected clients.

Remove the current implicit meanings where `defaultAgentId` means Codex-or-first and `activeProjectId` is treated as a remembered Project. Rename protocol fields as needed so a fallback is not presented as a persisted preference.

## Lifecycle

### Client initialize

`client/initialize` returns:

- current Project and Agent collections;
- persisted New Task defaults;
- the owning client's existing New Task snapshot, when one exists;
- ordered event cursor/baseline information.

Initialization must not create a New Task or launch an Agent. It may return an already persisted New Task.

### Open New Task with a cached instance

Frontend renders the cached New Task immediately. Clicking New Task, returning through browser history, or switching back from another Task performs no product request when the cache and event subscription remain continuous.

### Create the first New Task

When no New Task exists, Frontend renders the New Task surface in creating state and calls typed `task/create` with the selected `projectId` and `agentId`. The protocol gateway injects the initialized `clientInstanceId` into the workflow.

App Server performs one transaction or equivalently atomic mutation that:

1. returns the existing client-owned New Task if a concurrent duplicate request already created it;
2. otherwise validates Project and Agent availability;
3. creates a Task with private New Task lifecycle state and owner;
4. persists the Task identity before starting slow Agent work;
5. returns a snapshot with `preparation: preparing`;
6. starts Native Session acquisition asynchronously.

Two concurrent create requests for one client must return the same Task id. An existing New Task with different immutable context is a conflict; changing Project or Agent requires explicit discard followed by create.

### Agent preparation

App Server sends owner-scoped Task events as Native Session preparation changes. The snapshot/event projection may add or replace:

- Native Session preparation and readiness;
- configuration option catalog and pending option mutation state;
- slash-command catalog;
- attachment and message capabilities;
- send readiness;
- recoverable preparation errors.

Frontend applies only cursor-contiguous, monotonically newer state. The page remains rendered throughout preparation. Composer controls expose honest disabled or preparing states until their required capability is ready.

### Navigate away and return

Navigation changes only presentation. It must not call `task/discard`, clear the Task-owned local composer entry, release retained live attachment resources, close the Native Session, or remove the New Task snapshot.

Return renders cached state immediately. A reconnect or cursor gap installs a replacement owner-scoped snapshot; ordinary return navigation does not call `task/create` or `task/open` merely to prove the Task still exists.

### First send

`task/send` contains Task concurrency identity plus only user message content:

```text
taskId
message.text
message.attachmentHandleIds
```

Frontend issues `task/send` once. It does not automatically replay the request after timeout, disconnect, reconnect, reload, or unknown transport outcome. Remove the Send-specific idempotency key, durable browser pending-send record, request fingerprint retry contract, and exact automatic replay workflow. The envelope may retain its ordinary `clientRequestId` for request/response correlation and accepted-message provenance, but it is not a retry instruction and Frontend does not persist it across reload for replay.

The request does not resend Project, Agent, selected configuration values, option catalog, or slash-command catalog. Those already belong to the Task and Native Session.

When the user starts first Send, Frontend moves the draft into Task-scoped pending state and marks the New Task as submitting. If the user invokes New Task while Send is pending, Frontend reopens the same cached submitting instance. If cache loss causes it to ask App Server, normal client-scoped create/resolve semantics return that same New Task while it remains private. No queued navigation intent or concurrent replacement workflow exists. After authoritative Send acceptance makes the old Task visible, a later New Task action requests the next instance. Send rejection keeps the same New Task and recoverable draft. Transport loss enters connection-lost presentation and resynchronizes without replay.

The durable first-send transaction must:

1. acquire the per-Task command lock shared by every Send-relevant Task mutation;
2. read current Task state and validate client ownership, readiness, message shape, and attachment handles once under that lock;
3. durably append the user message and set Task state to `starting`;
4. change New Task lifecycle from client-private to visible;
5. update state-root last-used Project and Agent defaults;
6. publish the visible Task into Task Navigation and normal Task subscriptions;
7. release the command lock;
8. return the authoritative Task snapshot containing the accepted user message.

Promotion and message acceptance must be atomic from query readers' perspective. A failed validation keeps the same New Task private and preserves its composer. App Server does not contact ACP while holding the Task command lock.

App Server materializes the acceptance response from durable state before ACP prompt work. The accepted Task is `starting`, not `working`: the Agent has not received the prompt yet. After commit, App Server gives the Task's opaque Native Session handle, prompt, and attachments to `NativeSessionService.startPrompt` in background execution. That service owns whether the underlying ACP session is already live or must be loaded, resumed, or recreated; the Send workflow does not branch on those cases. When prompt execution actually begins, the Task becomes `working`. The primary `session/prompt` response ends `working` state, but it does not end the Native Session update subscription. Any definitive service or Agent failure after durable acceptance becomes a Task state transition delivered through snapshots/events; it is not retroactively returned as rejection of the accepted user message.

### Messages sent while working

A Send accepted while the Task is already `working` is a steering message. App Server durably appends it to Chat and returns the authoritative Task snapshot exactly like any accepted user message, then asks `NativeSessionService.steer` to forward it immediately to the same Native Session as an additional `session/prompt` request. The product workflow does not wait for that steering request's response, and a steering response never controls Task status. The protocol transport still consumes or safely discards any eventual JSON-RPC response.

The Native Session owns one update subscription from acquisition until close or replacement. Every `session/update` is processed in arrival order regardless of which prompt is active or has returned. Agent message chunks with the same Agent-owned `messageId` update the same in-progress Chat message; interleaved message ids remain separate. Tool updates use `toolCallId`. Prompt completion must not finalize, detach, cancel, or otherwise make this session update consumer reject later updates.

### Live Agent text presentation

Smooth text streaming is Frontend-only ephemeral presentation layered over immediately updated authoritative Chat state. Frontend keeps at most one Agent-message presenter and one Thought-message presenter for the currently opened Task. Each contains its selected message id, authoritative text, and currently visible text. A Task snapshot, initial open, Task switch, reconnect baseline, hidden browser tab, or reduced-motion preference renders all known text immediately and creates no animation backlog.

Only live `chatItemAppended` or `chatItemChunk` events received while the Task is open may advance a presenter. Agent text animates only when it belongs to the latest Agent text message in Chat; Thought text follows the same rule independently for the latest Thought message. “Latest” within either channel does not mean the last row in the mixed Chat timeline: tool, permission, question, Live Activity, or the other text channel does not disqualify it. When a newer message appears in a channel, Frontend flushes the previous message in that channel to authoritative text and moves that channel's presenter and caret to the newer message. It never animates two Agent messages or two Thought messages at once. Background Task events update cached authoritative state immediately without presentation animation, so opening that Task later shows all missed content at once.

Animation never gates, hides, delays, or reorders later Chat rows. Small frame-driven presenters reveal selected text toward its authoritative value; they own no protocol, Chat ordering, persistence, navigation, or Task lifecycle state. Only the selected latest message in each streamable text channel may show that channel's caret.

### Tool updates and details

The permanent Native Session listener owns current tool state by Native Session id plus Agent `toolCallId`. It merges every `tool_call_update` into that state and persists every accepted update even when no client is viewing the tool. Tool identity must not contain an OpenAIDE Turn id.

Separate lightweight shared product state from client-specific detail presentation:

- The normal Task subscription publishes a typed single-row upsert when visible tool summary state changes: identity, title, kind, status, short input summary, and any intentionally collapsed-visible output preview. It does not republish a complete Task snapshot, Project collection, or Task Navigation for a tool-row change.
- Large or hidden tool content remains in App Server-owned detail storage and is not included in ordinary Task snapshots or events.
- Expanding a tool creates a per-client tool-detail subscription. App Server immediately returns the latest stored detail and pushes later detail changes only to clients subscribed to that tool.
- Collapsing the tool removes only that client's detail subscription. Other clients may independently remain expanded and subscribed.
- A collapsed tool still receives lightweight summary and status changes because those remain visible and authoritative in Chat.
- App Server persists tool detail updates even when no detail subscriber exists, so a later expansion receives complete current content.

This replaces Frontend polling of running tool artifacts every 250 ms. Remove the current lossy 250 ms App Server suppression once tool summary publication is cheap; do not introduce coalescing unless measured update volume requires it. Any future coalescer must guarantee a trailing flush of the newest state.

Every defined ACP tool kind except `other` receives an appropriate distinct icon, action label, grouped summary classification, and detail presentation based only on fields actually supplied by the Agent. `think` is an ACP reasoning/planning tool call, distinct from `agent_thought_chunk`; present it with Thought-like visuals inside the activity group while preserving its tool identity, status, input, output, and updates. `other` retains the generic tool presentation.

Frontend always groups each uninterrupted run of Tool and Thought rows into one activity disclosure, collapsed when the group is first created. New adjacent rows extend the existing group and preserve its open state. User messages, Agent messages, permissions, questions, and other non-activity Chat rows end a group. Preserve every underlying Tool and Thought message id inside the presentation group so updates and Thought streaming still target the correct step.

### Permission requests

A pending ACP `session/request_permission` is transient workflow state, not Chat history. App Server keeps the active request and its Agent response channel in memory, changes the Task status to `waiting`, and delivers the request to eligible clients. A reconnecting client receives the still-active request again from that runtime state; App Server does not persist a pending Permission Chat row. Clients render the transient request after the latest Chat content while continuing to apply later session updates normally.

The first valid client response resolves the request for every client. App Server returns that outcome to the Agent and only then persists one resolved Permission Chat item, positioned with its associated Tool and Thought activity by `toolCallId` rather than by the transient request's former end-of-Chat position.

Prompt cancellation follows the same resolution path as a user decision. App Server responds to ACP with the required `cancelled` outcome, closes the transient request for every client, and persists a resolved Permission Chat item with a distinct cancellation message. Do not silently remove the request or introduce a separate cancellation lifecycle.

### Agent questions

ACP form elicitation uses the same transient-request lifecycle as permission. A pending Question is active App Server workflow state, not Chat history: set the Task to `waiting`, deliver or redeliver the request to eligible clients, render it transiently after current Chat, and continue accepting later session updates while it remains pinned there. Do not also persist a pending Question row.

Submit, user Cancel, and prompt cancellation all close the request for every client. App Server validates submitted values at the protocol boundary, returns the corresponding ACP accept or cancel response, and only then persists one resolved standalone Question Chat item. Frontend validation remains as immediate field feedback but is not authoritative. Replace the shared 50 ms waiter polling with a direct response signal.

### Deferred ACP Plan support

ACP `plan` session updates are currently discarded. Plan presentation and persistence are intentionally outside this refactor and may be designed in the future. Do not add partial Plan support while implementing this document.

### Live user-message chunks

Handle ACP `user_message_chunk` explicitly rather than through a catch-all branch. During live work, intentionally ignore it because App Server already persisted the user message before sending `session/prompt`; treating an Agent echo as new Chat content would duplicate that message. During `session/load`, continue using user-message chunks to reconstruct Native Session history, grouping chunks by native `messageId` when present. Replay must eventually preserve supported non-text user content instead of silently discarding it.

### Deferred legacy Session Modes

Dedicated ACP Session Modes remain available for compatibility but are deprecated in favor of Session Config Options and will be removed from ACP. OpenAIDE uses Config Options as its single product model. Legacy `modes`, `session/set_mode`, and `current_mode_update` support is outside this refactor; do not build a second synchronized Mode model. Record ignored legacy updates in diagnostics instead of silently accepting them.

### Deferred session usage

ACP `usage_update` is accepted by the currently enabled unstable schema feature but discarded by OpenAIDE. Context-window and Agent-reported cost presentation are intentionally deferred for a future design. Do not add partial usage state or UI in this refactor; diagnose ignored updates until support is designed.

### Primary prompt completion

Preserve the ACP `session/prompt` response and its `stopReason`; do not reduce every valid response to `Ok(())`. The primary prompt response changes the Task from `working` to idle but does not finalize Chat messages, Tool state, or the Native Session update stream.

- `end_turn` needs no additional Chat item.
- `max_tokens`, `max_turn_requests`, and `refusal` add an appropriate Live Activity explanation.
- `cancelled` completes the cancellation already initiated by the user without adding a duplicate result.
- A transport or protocol failure adds a failure Live Activity.

Remove the fixed 100 ms post-prompt update drain. The permanent Native Session listener continues accepting updates after the response and therefore needs no timing-based completion guess.

### Explicit user cancellation

Replace the existing cancellation implementation completely; do not retain its immediate-idle or Turn-scoped cleanup path as fallback compatibility behavior.

1. `task/cancel` changes the Task from `working` to `stopping` and immediately publishes that focused status change. Frontend disables Send while stopping.
2. App Server resolves every active transient Permission and Question through their normal cancellation resolution path, closes them for every client, persists their distinct resolved cancellation messages, and returns required cancellation outcomes to the Agent.
3. App Server sends ACP `session/cancel` through the Native Session service.
4. The permanent Native Session listener continues accepting all late Agent updates.
5. The Agent's primary prompt response with stop reason `cancelled` changes the Task from `stopping` to idle and persists exactly one `Task stopped` Live Activity.
6. Running Tool activity becomes interrupted/cancelled in the product model, never successfully `completed` merely because Stop was requested.
7. If cancellation or the Native Session definitively fails, leave `stopping`, publish a failure Live Activity, and expose explicit recovery instead of leaving the Task stuck.

Remove the old `active_turn_id`-based cancellation admission and cleanup, early `Inactive` transition, pre-Agent `Task was stopped` Chat insertion, and blanket completion of running activities.

### Native Session failure and restart

User Stop, Native Session failure during `starting`, `working`, or `waiting`, and App Server restart during active work use one shared termination pipeline. The pipeline closes transient requests for every client, persists cause-specific resolved request messages, marks unfinished Tool activity interrupted, ends active work, publishes idle state, and appends exactly one cause-specific Live Activity. Do not retain separate Turn recovery and failure cleanup implementations.

The causes still have different protocol behavior: user Stop sends ACP `session/cancel` and normally waits in `stopping` for the Agent's `cancelled` response; a lost Agent connection cannot receive cancellation responses, so App Server completes termination immediately and records that the Agent disconnected; restart records that OpenAIDE restarted. These differences must not create separate state-cleanup paths.

Never retry or replay the failed prompt automatically. Keep its already accepted User message. After termination, the user may Send normally; `NativeSessionService` may load or resume the Native Session for that new prompt but must never resend the previous prompt.

An unexpected Native Session loss while the Task is idle only marks the opaque handle unavailable. Keep the Task idle and do not add alarming Chat activity; the next explicit Send may make the service load or resume it.

## App Server publication model

Replace the current broad client event stream and snapshot fallback completely.

- `state/subscribe` returns one complete authoritative snapshot and its scope-local revision. Full snapshots are for initial subscription, reconnect, and explicit complete history replacement.
- Each subscribed scope has its own ordered stream. A Task subscription does not advance through unrelated Project, Agent, Navigation, or other Task events.
- Each durable Task transaction increments one Task revision and publishes exactly one `taskChanged` event for that revision. Its payload contains only the fields changed atomically by that transaction; do not split one commit into several independently ordered events.
- `taskChanged` may carry focused changes such as appended or upserted Chat items, appended text for a stable message id, status, lifecycle, title/summary, complete configuration catalog, complete command catalog, Send capability, history state, and other agreed Task fields.
- A missing next Task revision causes Frontend to discard that subscription replica and obtain one fresh baseline. Do not retry or reconstruct individual events.
- Transient Permission and Question delivery remains outside durable Task revisions until resolution persists a Chat item. Per-client Tool-detail subscriptions also remain separate from shared Task state.
- A complete history replacement is explicit and atomic; it replaces the subscription baseline rather than masquerading as a sequence of Chat deltas.

Remove `TaskUpdate.delta: Option<_>` fallback behavior, `taskSnapshotUpdated` publication after ordinary mutations, unrelated Project/Navigation broadcasts, and the broad cursor machinery that makes one scope consume unrelated events. Navigation receives a focused summary change only when a transaction actually changes Navigation-visible Task state.

### Configuration option updates

ACP `config_option_update` contains the complete current catalog. The permanent Native Session listener replaces the Task's stored catalog and selected values, then publishes a focused `configOptionsUpdated` event. Do not republish a complete Task snapshot, Chat, Project collection, or Navigation for this change. Frontend replaces its cached Task catalog with the event payload and rerenders Composer controls; configuration changes never create Chat rows.

User option changes remain serialized per Task. The complete catalog returned by `session/set_config_option`, or a later complete Agent update, is authoritative. Reconcile an Agent/user race from that latest catalog instead of exposing a race-only failure as user error.

### Available command updates

ACP `available_commands_update` contains the complete current slash-command catalog. The permanent Native Session listener replaces the Task's stored catalog and publishes a focused `availableCommandsUpdated` event. Frontend replaces its cached catalog for that Task and updates the Composer picker. Command catalog changes never create Chat rows and must not trigger complete Task, Chat, Project, or Navigation publication.

Remove the legacy behavior that represented catalog changes as fake “Updated slash commands” Chat activities. Do not preserve or migrate those legacy rows. Preserve all supported command-input information during normalization rather than silently reducing every command to name, description, and an unstructured hint.

Frontend remains on the New Task surface in submitting state until this acceptance response is reconciled. Because the UI permits only one in-flight Send per Task, a successful response clears that Task's composer directly and asks the App Shell to route/adopt the now-visible Task id. Do not match message text, message id, idempotency key, or a settlement key merely to clear the acknowledged composer. A rejected request leaves the composer unchanged. Frontend must not route to `/task/:id` merely because the request was issued.

First Send performs no Native Session history synchronization because the New Task has no Agent conversation history. Do not publish `historySync: syncing` merely because Send was accepted or background work was scheduled.

## History Synchronization

`NativeSessionCatalogService` owns cached `session/list` results keyed by Agent and Project Context. It supplies Native Sessions for client history surfaces and their `updatedAt` values. It refreshes active catalogs once per minute and on an explicit user Refresh request. Updating this catalog never initiates Task history synchronization.

Each Task persists `localHistoryUpdatedAt`, meaning the time when its stored Chat projection last changed. Advance it only when Chat content changes: accepting a user or steering message, persisting an Agent chunk or Chat activity, or replacing history after `session/load`. Do not advance it for opening or reading a Task, title changes, configuration options, commands, or unrelated Task metadata.

Opening an existing Task is the only automatic history-synchronization trigger. App Server reads the matching Native Session from the catalog cache and compares its Agent-provided `updatedAt` with `localHistoryUpdatedAt`. It performs no `session/list` request as part of Task open. App Server calls `session/load` only when the cached Native timestamp is present, comparable, and more than five seconds newer. This fixed tolerance absorbs the normal delay between App Server persisting a received Agent update and the Agent persisting its own session timestamp; it is not configurable and has no retry or stabilization machinery. If the Native timestamp is missing, invalid, equal, older, or no more than five seconds newer, App Server returns the stored Chat normally and performs no history operation. If it exceeds that threshold, App Server returns the stored Chat with `historySync: syncing`, disables Send, and loads the Native Session history in background. A successful replay becomes the complete Chat projection: App Server does not merge it with the previous local history. It atomically replaces the stored Chat with exactly the rendered `session/load` replay, sets `localHistoryUpdatedAt` to the load completion time, publishes a complete authoritative Task snapshot, ends the syncing state, and enables Send. A failed replay keeps the existing Chat, adds a `History update failed` Live Activity item, ends the syncing state, and enables Send; later Agent work proceeds normally against the Native Session.

Send never checks or initiates history synchronization. Catalog refresh never initiates it either. If the Native timestamp becomes newer while a Task remains open, synchronization waits until that Task is opened again; live updates for an App Server-owned Native Session continue through its permanent session update consumer instead.

If transport fails, Frontend does not retry. While the same Frontend process remains alive, it retains the local composer and reconnects/resubscribes. The authoritative snapshot determines whether the New Task became visible and contains the accepted message or remains private and unsent. After a full Frontend reload, no pending-send replay is attempted: an accepted message appears from App Server state, while an unaccepted memory-only draft may be lost as an explicit product trade-off.

### Explicit discard

Explicit discard validates owning client identity, closes or releases the empty Native Session safely, releases resolver resources, removes private New Task persistence, and clears only the matching local composer state after acknowledgement or an explicitly designed idempotent cleanup result.

## Persistence Model

Replace implicit `first_prompt_sent` and title-string lifecycle inference with explicit persisted state. The concrete Rust type may vary, but it must express the equivalent of:

```text
TaskLifecycle
  New { ownerClientInstanceId }
  Visible
```

New Task lookup must be indexed or otherwise transactionally unique by `clientInstanceId`. Do not locate a reusable New Task by scanning for matching Agent and workspace alone.

Persist one optional current Task title plus explicit provenance. New Tasks have no stored title by default; Frontend renders `New task` as presentation fallback. First Send does not invent a title. Agent metadata sets or clears only an Agent-owned title and never replaces a User-owned title. Native Session adoption stores the supplied session title as Agent-owned. User title mutation is a future interface and is not added implicitly as part of this flow.

Do not migrate existing records from the superseded lifecycle representation. New code reads and writes only the explicit lifecycle model.

## Query And Authorization Rules

- Task list, Task Navigation, Archive, search, support-facing normal Task counts, and cross-client state subscriptions exclude `TaskLifecycle::New`.
- Owner-scoped initialize and Task subscription may return the owner's New Task.
- Non-owner open, send, configure, attach, reveal, discard, or subscribe attempts return a stable authorization/not-found product error without revealing existence.
- App Server internal cleanup and support diagnostics may inspect New Tasks through explicitly internal queries.
- After promotion, normal Task authorization and subscription behavior applies.

## Frontend Modules

Do not implement the flow as more effects and refs in `useAppController`. Introduce cohesive modules with narrow interfaces:

```text
ShellRouter
  currentRoute()
  navigate(route)
  subscribe(listener)

NewTaskController
  state()
  open()
  create(selection)
  setConfigOption(configId, value)
  send(message)
  discard()

NativeSessionService
  acquire(agentId, projectContext)
  startPrompt(handle, message)
  steer(handle, message)
  setConfigOption(handle, configId, value)
  close(handle)

TaskDraftStore
  get(taskId)
  updateText(taskId, text)
  addAttachment(taskId, row)
  removeAttachment(taskId, rowId)
  markSending(taskId, attempt)
  reconcileSend(taskId, result)
  clear(taskId)
```

`NewTaskController` owns the New Task state machine and consumes App Server snapshots/events. `TaskDraftStore` owns local unsent composer state keyed by real Task id. The shared Frontend receives a shell-neutral typed route; Web URLs and VS Code panels stay in their App Shell adapters.

`NativeSessionService` is a deep App Server module. `acquire` returns an opaque handle with one session update consumer already connected to Task projection. `startPrompt` uses that handle and internally returns an existing live ACP session or performs required load/resume/recreate work. Task Send never inspects Native Session readiness fields or chooses an ACP recovery method. `steer` forwards additional user messages without making their responses part of Task status. The session consumer persists assistant chunks, tool activity, permissions, terminal state, title, options, and commands for the lifetime of the handle rather than being attached to individual prompts.

Suggested New Task presentation phases:

```text
absent
creating
preparing(taskId)
ready(taskId)
sending(taskId)
connectionLost(taskId)
failed(taskId?, recoverableError)
```

Avoid parallel booleans that can represent contradictory combinations.

## Attachments

Frontend owns visible row order and presentation metadata. App Server owns opaque resolver resources, safe file validation, allowed-root enforcement, delivery conversion, single-use consumption, and client/Task authorization.

- Attachment creation requires the real Task id and owning client connection.
- Frontend caches safe row metadata plus opaque handle ids; it never treats paths or file bytes as authoritative.
- Same-client live navigation retains rows and server resources.
- `task/send` submits handle ids only.
- Successful send consumes handles; failed validation keeps them usable; explicit row removal or discard releases them.
- Full-reload reconstruction of unsent attachment rows is outside this plan. Preserve the existing security rule that handles are not reload-durable unless a separate design explicitly introduces a client-private attachment manifest.

## Options And Slash Commands

Agent-provided catalogs are authoritative Task projections. Frontend caches them for rendering and sends dedicated option-change intents; it never includes catalogs or selected option values in `task/send`.

Keep option race behavior simple:

1. do not optimistically publish the requested value as authoritative;
2. show the changed control as pending;
3. serialize user option requests for one Native Session;
4. apply Agent response and independent Agent notification catalogs through one monotonically ordered App Server revision stream;
5. ignore stale response snapshots in Frontend;
6. reconcile a user request superseded by newer Agent state as a successful newest-catalog result, not a generic user error;
7. show errors only for genuine transport, setup, authorization, unsupported-operation, or Agent failures.

Slash-command catalogs follow the same snapshot/event ordering. A command catalog is composer assistance, not extra `task/send` state.

## Implementation Sequence

Each slice starts with a failing boundary test and ends with narrow checks before broader CI.

1. **Protocol and domain types**
   - Add explicit New Task lifecycle/ownership and default-selection records.
   - Extend initialize and Task snapshots/events with owner-scoped New Task state.
   - Regenerate TypeScript bindings and run protocol checks.
2. **Storage replacement**
   - Persist lifecycle ownership, unique client lookup, title provenance, and global last-used defaults.
   - Add atomic promotion, crash, and reload tests for the new format only.
3. **App Server create/query authorization**
   - Make create client-aware and idempotent per client.
   - Filter New Tasks from all normal collections and deny non-owner access.
4. **Preparation and owner-scoped events**
   - Retain asynchronous Native Session preparation while routing state only to the owner.
5. **Atomic first-send promotion**
   - Serialize through one per-Task command lock; validate once; promote visibility, commit the user message and starting Task state, consume handles, publish navigation, and update defaults atomically before ACP work.
6. **Frontend shell routing seam**
   - Move Web routing and VS Code routing into injected App Shell adapters.
7. **Frontend New Task controller and draft store**
   - Replace navigation-driven discard, durable browser Send replay, and split composer ownership with the accepted state machine, memory cache, reconnect/resync behavior, and no automatic product-request replay.
8. **Option and command reconciliation**
   - Unify ordering and superseded non-error behavior.
9. **Remove legacy paths**
   - Delete global reusable-empty-task lookup, magic title sentinel, navigation discard effects, and obsolete reducer actions/refs.
10. **End-to-end verification**
   - Verify Web and VS Code, desktop and narrow/mobile layouts, reconnect/reload, concurrent clients, first-send rejection, connection loss without replay, and cache/event resynchronization.

## Required Boundary Tests

- First create returns a Task id before Agent preparation completes.
- Two concurrent creates from one client return the same New Task.
- Clicking New Task while first Send is unresolved reopens the same submitting New Task; after acceptance, a later click creates a different New Task.
- Different clients receive different New Tasks for identical Project and Agent choices.
- Same-client reconnect/initialize returns the same New Task.
- New Task is absent from every normal Task list, navigation, archive, search, and other-client snapshot.
- Non-owner open, configure, attach, send, subscribe, and discard cannot reveal or mutate it.
- Navigation to Task, Settings, browser Back/Forward, and return preserves text, attachment rows, Task id, and Native Session.
- Project and Agent are fixed after creation; replacement requires explicit discard.
- Preparation, option, command, and capability events reach only the owner and advance monotonically.
- Benign Agent/user option races reconcile to the newest catalog without a generic user error.
- First send includes no option or command catalog fields.
- First send atomically promotes the Task into navigation exactly once.
- Lost first-send response causes reconnect/resync without replay; accepted state appears from App Server, while an unaccepted live draft remains available only in the still-running Frontend.
- Opening a Task never calls `session/list`; it uses the catalog service's cached Native timestamp.
- Missing, invalid, equal, older, or no-more-than-five-seconds-newer Native timestamps do not call `session/load` and leave stored Chat unchanged.
- A cached Native timestamp more than five seconds newer calls `session/load` once, blocks Send only while loading, and atomically replaces Chat with exactly the replayed history without merging.
- A failed history load preserves Chat, records `History update failed` as Live Activity, and re-enables Send.
- Validation failure preserves private New Task and composer resources.
- Explicit discard cleans up exactly once.
- App Server restart restores client ownership and New Task defaults without exposing private state.

## Verification Commands

Run the narrowest affected tests first, then:

```text
npm run protocol:generate
npm run protocol:check
cargo fmt --all --check
cargo test -p openaide-app-server
cargo clippy -p openaide-app-server --all-targets -- -D warnings
npm run check
npm run test --workspaces --if-present
npm run ci
```

Before commit, inspect the staged diff for secrets, local paths, personal domains, usernames, email addresses, credentials, and machine-specific configuration. Push only through a feature branch and pull request.
