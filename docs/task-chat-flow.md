# Task Lifecycle and Chat Specification

Status: accepted product and architecture specification

This document is the source of truth for creating a New Task, sending messages, running its Native Session, projecting Agent updates into Chat, and recovering the Frontend replica after connection loss. It defines the required behavior, ownership, and invariants.

This specification is accepted. Any proposal to change, weaken, or expand its behavior requires discussion with the user and explicit agreement before the specification or implementation changes.

The detailed architecture decisions supporting this specification are:

- [Task state publication and replica recovery](adr/0023-task-state-publication-and-replica-recovery.md);
- [Task Chat persistence](adr/0024-task-chat-persistence.md);
- [Task Frontend boundaries](adr/0025-task-frontend-boundaries.md).

## Design Constraints

- Every mechanism protects a named product invariant or a demonstrated common failure.
- Prefer one owner, one state representation, one ordering mechanism, and one validation pass.
- Prefer visible failure with explicit recovery over transparent mutation retry.
- Simplicity includes caller knowledge, invalid state combinations, duplicated policy, hidden ordering, and cross-module coordination; it is not measured only by line count.
- One accepted user action creates one User message, an acknowledged message survives restart, and content cannot reach the wrong client, Task, or Native Session.
- A requirement that conflicts with or falls outside this specification requires discussion before the design expands.

OpenAIDE provides no compatibility guarantee for superseded development-only state, protocol shapes, or persisted data. The accepted model is the only product model; compatibility adapters, fallback deserialization, and speculative migrations are outside its scope.

## Outcome

OpenAIDE keeps at most one client-private New Task for each stable `clientInstanceId`. The New Task has a real Task id and acquires a real Agent Native Session, but remains invisible outside its owning client until App Server durably accepts its first User message. Navigation retains it. While first Send is unresolved, invoking New Task reopens the same submitting instance. The first accepted Send promotes the same Task identity into normal visible Task state; a later New Task action creates the next instance.

## Vocabulary And Ownership

- **New Task** is the canonical term. `Draft Task`, `Established Task`, and `slot` are not product or interface terms.
- A New Task is private to one `clientInstanceId`, and one client owns at most one New Task.
- Project Context and Agent are selected before New Task creation and become immutable when its Native Session is created.
- A New Task and a Task with first Send in flight are excluded from Task Navigation, active and archived Task lists, normal history and session discovery, search, and other clients' snapshots and events.
- Ordinary navigation, view unmount, and switching to Settings or an existing Task retain the New Task and its Native Session. Only explicit discard removes it before first Send.
- The first durably accepted User message atomically makes the same Task visible through normal Task queries and events.
- App Server owns Task identity, Native Session state, options, commands, readiness, capabilities, attachment resolver resources, first-Send promotion, and durable Chat.
- Frontend owns unsent text, visible attachment-row presentation, and ephemeral streaming presentation.
- **Native Session update consumer** is the canonical name for the one session-lifetime listener that projects `session/update` notifications into Task state.
- **Baseline** is a complete authoritative snapshot for one subscribed scope at one scope-local revision.

## Client Identity

Frontend supplies `clientInstanceId` only through `client/initialize`. Transport assigns a connection-local `connectionId`; `ClientHub` maps that connection to the initialized client. Product handlers obtain client identity from this trusted connection context instead of accepting a client id in product request parameters.

- Every browser tab owns a distinct `clientInstanceId` and retains it across reload through session-scoped storage with memory fallback. A newly opened or duplicated tab receives a distinct identity even if browser storage was copied.
- VS Code and other native shells issue a stable identity for the shell client or webview lifecycle.
- Reconnect sends the same `clientInstanceId` through `client/initialize`.
- Every transport connection receives a fresh connection-local `connectionId`; transport identity never reuses `clientInstanceId`.
- A client that loses its stable identity becomes a new client and cannot recover another client's New Task.

## Default Project And Agent

There are two selection owners:

1. A live client remembers its last selected Project and Agent locally.
2. App Server persists state-root-wide last-used Project and Agent defaults only as the initial selection for a client without a retained selection.

Selection priority is:

1. the client's retained valid selection;
2. an explicit App Shell Project hint, such as the VS Code Project Context;
3. App Server's persisted last-used default when it remains available;
4. the first available Project in collection label/id order and the first available Agent in registry order.

App Server returns the global defaults in the initialized client snapshot. Frontend adopts them only when it has no retained choice and validates them against the Project and Agent collections. The first successful Send for a New Task updates the state-root defaults to that Task's Project and Agent. Connected clients retain their own selections, and no separate preference request is involved. Protocol fields distinguish persisted defaults from deterministic collection fallbacks.

## New Task Lifecycle

### Client initialization

`client/initialize` returns:

- Project and Agent collections;
- persisted New Task defaults;
- the owning client's existing New Task snapshot, when one exists;
- event-stream and baseline information required to establish subscriptions.

Initialization never creates a New Task or launches an Agent. It may return an already persisted New Task.

### Opening a cached New Task

Frontend renders the cached New Task immediately. Clicking New Task, returning through browser history, or switching back from another Task performs no product request while the cache and event subscription remain continuous.

### Creating a New Task

When no New Task exists, Frontend renders the New Task surface in `creating` state and calls typed `task/create` with the selected `projectId` and `agentId`. The protocol gateway supplies the initialized client identity from the connection context.

App Server performs one atomic mutation that:

1. returns the existing client-owned New Task when a concurrent duplicate request already created it;
2. otherwise validates Project and Agent availability;
3. creates a Task with private New Task lifecycle state and owner;
4. persists the Task identity before slow Agent work starts;
5. returns a snapshot with `preparation: preparing`;
6. starts Native Session acquisition asynchronously.

Two concurrent create requests for one client return the same Task id. An existing New Task with different immutable context is a conflict; changing Project or Agent requires explicit discard followed by create.

### Agent preparation

App Server sends owner-scoped Task events as Native Session preparation changes. The projection may update:

- Native Session preparation and readiness;
- the configuration-option catalog and pending option mutation state;
- the slash-command catalog;
- attachment and message capabilities;
- Send readiness;
- recoverable preparation errors.

Frontend applies only contiguous, monotonically newer Task revisions. The page remains rendered throughout preparation, and Composer controls show honest disabled or preparing states until their required capability is ready.

App Server Send capability contains authoritative readiness and blockers. Frontend combines it with local draft content and attachment-handle validity through one shared Composer availability model. ACP has no text-required prompt capability: a completely empty message is invalid, while attachment-only input is valid when every attachment resolves to an Agent-supported ACP content block.

### Navigation and discard

Navigation changes only presentation. The client retains the New Task snapshot, Task-scoped composer entry, live attachment resources, and Native Session. Returning renders cached state immediately and does not call `task/create` or `task/open` merely to prove that the Task still exists. A reconnect or revision gap installs a replacement owner-scoped baseline according to [ADR-0023](adr/0023-task-state-publication-and-replica-recovery.md).

Explicit discard validates the owning client, closes or releases the empty Native Session safely, releases resolver resources, removes private New Task persistence, and clears only the matching local Composer state after acknowledgement or an explicitly designed idempotent cleanup result.

## Send

### Request shape and replay rule

`task/send` contains Task identity plus User message content:

```text
taskId
message.text
message.attachmentHandleIds
```

The request does not resend Project, Agent, configuration values or catalogs, or the slash-command catalog. Those already belong to the Task and Native Session.

Frontend issues each `task/send` mutation once and never automatically replays it after timeout, disconnect, reconnect, reload, or an unknown transport outcome. An ordinary `clientRequestId` may correlate the request and response and record accepted-message provenance, but it is not a retry instruction and is not persisted across reload for replay. Frontend retries only the read-only event stream as defined by [ADR-0023](adr/0023-task-state-publication-and-replica-recovery.md).

### First Send

Frontend moves the Composer draft into Task-scoped pending state and marks the New Task as submitting. Invoking New Task while Send is pending reopens that same cached submitting instance. If cache loss requires App Server resolution, client-scoped create semantics return that instance while it remains private. After authoritative acceptance makes the Task visible, a later New Task action requests the next instance. Rejection keeps the same New Task and recoverable draft. Transport loss enters connection-lost presentation and resynchronizes without replay.

The durable first-Send transaction:

1. acquires the per-Task command lock shared by every Send-relevant Task mutation;
2. reads Task state and validates client ownership, readiness, message shape, and attachment handles once under that lock;
3. durably appends the User message and sets Task state to `starting`;
4. changes New Task lifecycle from client-private to visible;
5. updates state-root last-used Project and Agent defaults;
6. publishes the visible Task into Task Navigation and normal Task subscriptions;
7. releases the command lock;
8. returns the authoritative Task snapshot containing the accepted User message.

Promotion and message acceptance are atomic for query readers. Failed validation keeps the same New Task private and preserves its Composer. App Server performs no ACP I/O while holding the Task command lock.

App Server materializes the acceptance response from durable state before ACP prompt work. The accepted Task is `starting`, because the Agent has not received the prompt. After commit, App Server gives the Task's opaque Native Session handle, prompt, and attachments to `NativeSessionService.startPrompt` in background execution. The service owns whether the underlying ACP session is live or must be loaded, resumed, or recreated. When prompt execution actually begins, the Task becomes `working`. A definitive service or Agent failure after durable acceptance becomes a Task state transition; it does not retroactively reject the accepted User message.

Frontend remains on the New Task surface in submitting state until it reconciles acceptance. Because the UI permits only one in-flight Send per Task, success clears that Task's Composer directly and asks the App Shell to route to the now-visible Task id. It does not compare message text, message id, idempotency key, or a settlement key to clear the acknowledged Composer. Rejection leaves the Composer unchanged and does not route to the Task page.

First Send performs no history synchronization because the New Task has no Agent conversation history. Acceptance or scheduling alone never publishes `historySync: syncing`.

### Steering messages

A Send accepted while the Task is `working` is a steering message. App Server durably appends it to Chat and returns authoritative Task state, then asks `NativeSessionService.steer` to forward it to the same Native Session as another `session/prompt` request. The workflow does not wait for the steering response, and that response never controls Task status. The transport consumes or safely discards any eventual JSON-RPC response.

## Native Session Updates And Chat

### Update-consumer lifetime

One Native Session update consumer exists from acquisition until session close or replacement. It processes every `session/update` in arrival order regardless of which prompt is active or has returned. Prompt completion never finalizes, detaches, or narrows this consumer.

Agent message chunks with the same Agent-owned `messageId` update the same in-progress Chat message; interleaved message ids remain separate. Tool updates use `toolCallId` and never depend on an OpenAIDE Turn id.

### Live Agent and Thought text

Smooth streaming is Frontend-only ephemeral presentation layered over immediately updated authoritative Chat. The authoritative Task reducer owns one Chat replica, not a second presented-Chat array. Only the selected Agent or Thought row owns an ephemeral visible-text cursor. Initial open, Task switch, a baseline, a hidden browser tab, and reduced-motion preference render all known text immediately without an animation backlog.

Only live Chat append or chunk events received while the Task is open may advance a presenter. Agent text animates only for the latest Agent text message; Thought text independently animates only for the latest Thought message. “Latest” within a channel does not mean the final row in the mixed timeline. When a newer message appears in one channel, Frontend flushes the previous message in that channel and moves its presenter and caret to the newer message. Background Task events update authoritative cached state without animation.

Animation never gates, hides, delays, or reorders later Chat rows. Frame-driven presenters catch up within 96 ms of the newest update regardless of chunk size. Only the selected latest message in each channel shows its caret. The live-event signal and authoritative Task change enter the reducer as one action so one server event causes one root state transition.

Protocol-to-view mapping preserves object identity for unchanged Chat items and derived uninterrupted Tool and Thought groups. Task Navigation changes only when a navigation-visible summary field changes. The Chat viewport observes direct row insertion and row size changes; row-level `ResizeObserver`s handle later Markdown, image, request, and Tool-detail reflow.

### Tool activity and details

The Native Session update consumer owns tool state by Native Session id plus Agent `toolCallId`, merges every accepted `tool_call_update`, and persists it even when no client views the Tool.

- The Task subscription publishes lightweight visible Tool summary changes: identity, title, kind, execution status, short input summary, collapsed-visible output preview, and lightweight permission outcomes.
- Large or hidden content remains in App Server-owned detail storage and is absent from ordinary Task snapshots and events.
- Expanding a Tool creates a per-client detail subscription that returns the latest stored detail and pushes later changes.
- Collapsing a Tool removes only that client's detail subscription. Lightweight summary and status changes remain visible through the Task subscription.
- App Server persists detail changes without subscribers so later expansion receives complete content.

Tool detail delivery is event-driven and does not poll. Publication is not coalesced unless measured volume requires it; any future coalescer guarantees a trailing flush of the newest state.

Every defined ACP Tool kind except `other` has a distinct appropriate icon, action label, grouped-summary classification, and detail presentation based only on Agent-supplied fields. ACP `think` is a reasoning or planning Tool call, distinct from `agent_thought_chunk`; it uses Thought-like visuals inside an activity group while retaining Tool identity, status, input, output, and updates. `other` uses the generic presentation.

Frontend groups each uninterrupted run of Tool and Thought rows into one activity disclosure, collapsed when created. Adjacent activity rows extend the group while preserving its open state. User, Agent, Permission, Question, and Live Activity rows end the group. The group preserves every underlying message id.

Expanded groups hide Thought rows by default and expose one leading control with the hidden count. Showing reasoning restores Thoughts at their original positions among Tools. This presentation does not affect `think` Tool calls.

### Permission requests

A pending ACP `session/request_permission` is transient workflow state, not Chat history. App Server keeps the active request and Agent response channel in memory, changes Task status to `waiting`, and delivers or redelivers the request to eligible clients. Clients pin it after the latest Chat content while continuing to apply later session updates.

The first valid client response closes the request for every client. App Server returns the outcome to the Agent and durably appends it to the exact Tool identified by Native Session plus `toolCallId`; it creates no resolved Permission Chat row. Tool execution status remains Agent-owned and independent of permission outcome. A Tool may receive multiple permission requests, and every decision remains visible in its details.

Prompt cancellation uses this same resolution path with the ACP `cancelled` outcome and a cancelled decision on the linked Tool.

### Agent questions

ACP form elicitation uses the same transient-request lifecycle. A pending Question is memory-owned workflow state: Task status becomes `waiting`, eligible clients receive or receive again the request, and clients pin it after current Chat while continuing to apply session updates. Pending Questions are absent from durable Chat.

Submit, user Cancel, and prompt cancellation close the request for every client. App Server authoritatively validates submitted values, returns the ACP accept or cancel response, and then persists one resolved standalone Question Chat item. Frontend validation provides immediate field feedback only. The Agent response channel is signaled directly rather than polled.

### User-message chunks

During live work, App Server intentionally ignores ACP `user_message_chunk` because it already persisted the User message before `session/prompt`; an Agent echo must not duplicate it. During `session/load`, user-message chunks reconstruct Native Session history, grouped by native `messageId` when present, including supported non-text content.

### Primary prompt completion

App Server preserves the ACP `session/prompt` response and `stopReason`. The primary response changes Task status from `working` to idle but does not finalize Chat messages, Tool state, or the Native Session update consumer.

Before publishing that idle transition, App Server projects every session update already received ahead of the prompt response through the same ordered Task revision stream. Session updates received after the response remain valid and continue through the session-lifetime update consumer.

- `end_turn` adds no Chat item.
- `max_tokens`, `max_turn_requests`, and `refusal` add an appropriate Live Activity explanation.
- `cancelled` completes user-initiated cancellation without a duplicate result.
- transport or protocol failure adds a failure Live Activity.

### User cancellation and termination

`task/cancel` changes a working Task to `stopping`, publishes that status, and disables Send. App Server resolves every active transient Permission and Question through its normal cancellation path, sends ACP `session/cancel` through the Native Session service, and continues accepting late session updates. The primary prompt's `cancelled` response changes the Task to idle and persists exactly one `Task stopped` Live Activity. Running Tool activity becomes interrupted or cancelled, never successfully completed merely because Stop was requested. A definitive cancellation or Native Session failure leaves `stopping`, publishes failure activity, and exposes explicit recovery.

User Stop, Native Session failure during `starting`, `working`, or `waiting`, and App Server restart during active work share one termination pipeline. It closes transient requests, persists cause-specific resolved request messages, marks unfinished Tool activity interrupted, ends active work, publishes idle state, and appends exactly one cause-specific Live Activity. Protocol behavior still differs by cause: user Stop sends `session/cancel` and normally waits for `cancelled`; Agent disconnection terminates immediately; restart records that OpenAIDE restarted.

Accepted prompts are never replayed automatically after termination. Their User messages remain durable. A later explicit Send may load or resume the Native Session, but it never resends the failed prompt. Unexpected Native Session loss while idle only marks the opaque handle unavailable; the next explicit Send may recover it without alarming Chat activity.

### ACP update scope

- ACP `plan` updates are ignored until Plan presentation and persistence are deliberately specified.
- OpenAIDE uses Session Config Options as its single product model. Dedicated Session Modes are ignored with diagnostics rather than synchronized into a second Mode model.
- ACP `usage_update` is ignored with diagnostics until context-window and Agent-reported cost presentation are specified.

## History Synchronization

Opening an existing Task is the only automatic history-synchronization trigger. It compares the cached Agent-provided Native Session timestamp with the Task's durable `localHistoryUpdatedAt`; neither Send nor catalog refresh initiates synchronization. Exact catalog, tolerance, replacement, failure, and publication behavior is defined by [ADR-0023](adr/0023-task-state-publication-and-replica-recovery.md), while timestamp persistence is defined by [ADR-0024](adr/0024-task-chat-persistence.md).

When synchronization is required, Frontend keeps the stored Chat visible, shows `historySync: syncing`, and disables Send. A successful `session/load` atomically replaces Chat with exactly the rendered replay. Failure keeps existing Chat, appends `History update failed` Live Activity, ends syncing, and enables Send.

## Task Titles

A Task stores one optional title with Prompt, Agent, or User provenance. New Tasks have no stored title; Frontend renders `New task` as fallback. First Send stores a normalized 60-character prefix of the first User message as a provisional Prompt title. An Agent title value or clear supersedes Prompt- or Agent-owned state, while User-owned titles remain protected. Native Session adoption stores the supplied session title as Agent-owned. User title mutation remains a future interface.

## Query And Authorization

- Task lists, Task Navigation, Archive, search, ordinary support-facing Task counts, and cross-client subscriptions exclude New Tasks.
- Owner-scoped initialization and Task subscription may return the owner's New Task.
- Non-owner open, Send, configure, attach, reveal, discard, and subscribe intents return one stable authorization or not-found error without revealing existence.
- Explicitly internal cleanup and support diagnostics may inspect New Tasks.
- After promotion, normal visible-Task authorization and subscription rules apply.

## Attachments

Frontend owns visible row order and presentation metadata. App Server owns opaque resolver resources, safe file validation, allowed-root enforcement, delivery conversion, single-use consumption, and client and Task authorization.

- Attachment creation requires the real Task id and owning client connection.
- Frontend caches safe row metadata plus opaque handle ids; paths and file bytes are never authoritative Frontend state.
- Same-client live navigation retains rows and resolver resources.
- `task/send` submits handle ids only.
- Successful Send consumes handles; failed validation keeps them usable; row removal or New Task discard releases them.
- Unsent attachment rows are not reconstructed after full reload. Handles are not reload-durable unless a separate specification introduces a client-private attachment manifest.

## Options And Slash Commands

Agent-provided catalogs are authoritative Task projections. Frontend caches them for rendering and sends dedicated option-change intents; `task/send` contains neither catalogs nor selected option values. Catalog publication follows [ADR-0023](adr/0023-task-state-publication-and-replica-recovery.md).

Option changes follow one ordering model:

1. Frontend marks the changed control pending without publishing the requested value as authoritative.
2. App Server serializes user option requests for one Native Session.
3. Agent responses and independent Agent notifications enter one monotonically ordered Task revision stream.
4. Frontend applies the newest complete catalog and ignores stale response state.
5. A user request superseded by newer Agent state resolves to the newest catalog without a race-only user error.
6. Visible errors are reserved for genuine transport, setup, authorization, unsupported-operation, or Agent failures.

Slash-command catalogs use the same snapshot and event ordering. They provide Composer assistance and add no state to `task/send` or Chat.

## Conformance Invariants

An implementation conforms to this specification only when all of these are true:

1. One stable client owns at most one private New Task, and ordinary navigation retains it.
2. First-Send message acceptance and New Task promotion are one durable atomic mutation.
3. Each Send mutation is issued once and is never automatically replayed.
4. One Native Session update consumer survives prompt completion and accepts later updates until session close or replacement.
5. One durable Task transaction produces one ordered Task revision; a revision gap installs one new baseline.
6. Connection recovery retries only the event stream, then installs exactly one baseline for each active scope before Send is enabled.
7. Durable Chat, transient requests, Tool details, and Frontend-only presentation each have one explicit owner and do not masquerade as one another.
