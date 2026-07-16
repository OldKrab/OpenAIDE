# Task Lifecycle and Chat Specification

Status: accepted product and architecture specification

This document is the source of truth for creating a New Task, sending messages, running its Native Session, projecting Agent updates into Chat, and recovering the Frontend replica after connection loss. It defines the required behavior, ownership, and invariants.

This specification is accepted. Any proposal to change, weaken, or expand its behavior requires discussion with the user and explicit agreement before the specification or implementation changes.

The detailed architecture decisions supporting this specification are:

- [Task state publication and replica recovery](adr/0023-task-state-publication-and-replica-recovery.md);
- [Task Chat persistence](adr/0024-task-chat-persistence.md);
- [Task Frontend boundaries](adr/0025-task-frontend-boundaries.md);
- [Task Attention state and shell-local notifications](adr/0027-task-attention-and-shell-local-notifications.md).

## Design Constraints

- Every mechanism protects a named product invariant or a demonstrated common failure.
- Prefer one owner, one state representation, one ordering mechanism, and one validation pass.
- Prefer visible failure with explicit recovery over transparent mutation retry.
- Simplicity includes caller knowledge, invalid state combinations, duplicated policy, hidden ordering, and cross-module coordination; it is not measured only by line count.
- One accepted user action creates one User message, an acknowledged message survives restart, and content cannot reach the wrong client, Task, or Native Session.
- A requirement that conflicts with or falls outside this specification requires discussion before the design expands.

OpenAIDE provides no compatibility guarantee for superseded development-only state, protocol shapes, or persisted data. The accepted model is the only product model; compatibility adapters, fallback deserialization, and speculative migrations are outside its scope.

## Outcome

OpenAIDE maintains a bounded pool of durable zero-message Prepared Tasks keyed by `(Agent, canonical Task Workspace folder)`. A client acquires an exclusive lease on one matching Prepared Task while using New Task. Ordinary navigation retains that lease; changing Project, Agent, or Task Workspace releases it while preserving the Frontend-owned composer. The first accepted Send promotes the same Prepared Task and Native Session into normal visible Task state.

## Vocabulary And Ownership

- **New Task** is the canonical term. `Draft Task`, `Established Task`, and `slot` are not product or interface terms.
- **Prepared Task** is a durable zero-message New Task with its own Task id and Agent Native Session. It remains excluded from Task Navigation, active and archived Task lists, normal history and session discovery, and search until first Send is accepted.
- **Prepared-Task lease** is exclusive use of one Prepared Task by one initialized `clientInstanceId`. One client holds at most one lease, and one Prepared Task is leased to at most one client.
- **Free Prepared Task** is a ready, unleased, zero-message Prepared Task eligible for reuse by a matching pool key.
- Project Context, Agent, and Task Workspace form the selected New Task context. Changing any member releases the previous lease before acquiring the new context.
- Ordinary navigation, view unmount, and switching to Settings or an existing Task retain the lease and Native Session.
- The first durably accepted User message atomically makes the same Prepared Task visible through normal Task queries and events.
- App Server owns Task identity, Prepared-Task leases, pool policy, Native Session state, options, commands, readiness, capabilities, first-Send promotion, and durable Chat.
- Frontend owns the unsent composer: prompt text, `@file` mention text, Image bytes and previews, and ephemeral streaming presentation.
- **Native Session update consumer** is the canonical name for the one session-lifetime listener that projects `session/update` notifications into Task state.
- **Baseline** is a complete authoritative snapshot for one subscribed scope at one scope-local revision.

## Client Identity

Frontend supplies `clientInstanceId` only through `client/initialize`. Transport assigns a connection-local `connectionId`; `ClientHub` maps that connection to the initialized client. Product handlers obtain client identity from this trusted connection context instead of accepting a client id in product request parameters.

- Every browser tab owns a distinct `clientInstanceId` and retains it across reload through session-scoped storage with memory fallback. A newly opened or duplicated tab receives a distinct identity even if browser storage was copied.
- VS Code and other native shells issue a stable identity for the shell client or webview lifecycle.
- Reconnect sends the same `clientInstanceId` through `client/initialize`.
- Every transport connection receives a fresh connection-local `connectionId`; transport identity never reuses `clientInstanceId`.
- A client that loses its stable identity becomes a new client and cannot use another client's Prepared-Task lease.

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
- event-stream and baseline information required to establish subscriptions.

Initialization never acquires a Prepared Task or launches an Agent. The Prepared-Task pool adds no initialization field and does not use `requestedSurface` as an ownership or recovery input.

### Opening a cached New Task

Frontend renders its cached leased Prepared Task immediately. Clicking New Task, returning through browser history, or switching back from another Task performs no product request while the cache and event subscription remain continuous.

### Acquiring a Prepared Task

When its live New Task composer needs a Prepared Task, Frontend renders the preparation state and calls typed `task/acquire` with the selected Project, Agent, and Task Workspace identity. App Server derives the canonical workspace and pool key; Frontend never supplies a canonical pool key. The protocol gateway supplies initialized client identity from the trusted connection context.

App Server serializes acquisition and:

1. returns the same Prepared Task when that client already leases the requested key;
2. rejects acquisition of a different key until the client's previous release is acknowledged;
3. validates Project, Agent, and Task Workspace availability;
4. atomically leases the ready free Prepared Task for that key when one exists;
5. otherwise persists and leases a new zero-message Task before slow Agent work starts;
6. returns its ordinary Task snapshot, with `preparation: preparing` when Native Session acquisition is still running.

Concurrent duplicate acquisition requests for one client and key return the same Task id. Only ready, unleased, zero-message Tasks are reusable; a missing free entry creates and prepares another Task.

### Free-pool retention and recovery

Releasing a ready Prepared Task retains it only when its key has no other free entry. A second released entry for the same key is disposed and its Native Session is closed. Retained free entries are bounded by one internal global cap. A retained release becomes the newest free entry; when the cap is exceeded, App Server evicts the free entry that has waited longest, using Task id only to break equal-time ties.

Free entries, pool keys, counts, recency, and eviction decisions are App Server-internal. Normal diagnostics record Task id, lifecycle outcome, reason, and aggregate counts without workspace paths, client ids, prompts, or Agent configuration values.

App Server restart clears all leases before accepting requests. Durable Prepared Tasks remain; ready zero-message records rebuild the free pool, and their Native Sessions are restored lazily only when leased. Existing legacy zero-message New Tasks are adopted as free candidates with owners cleared; App Server keeps the newest eligible entry per key, applies the global cap, and closes extras. It never restores a pre-restart lease.

Disabling or deleting an Agent disposes its free and leased zero-message Prepared Tasks. Releasing a failed Prepared Task also disposes it because failed entries are never free-pool candidates.

### Agent preparation

App Server sends lessee-scoped Task events as Native Session preparation changes. The projection may update:

- Native Session preparation and readiness;
- the configuration-option catalog and pending option mutation state;
- the slash-command catalog;
- Image and message capabilities;
- Send readiness;
- recoverable preparation errors.

Frontend applies only contiguous, monotonically newer Task revisions. The page remains rendered throughout preparation, and Composer controls show honest disabled or preparing states until their required capability is ready.

App Server Send capability contains authoritative readiness and blockers. Frontend combines it with local draft content and Image compatibility through one shared Composer availability model. ACP has no text-required prompt capability: a completely empty message is invalid, while Image-only input is valid when the selected Agent accepts Image content.

### Navigation and release

Navigation changes only presentation. The client retains the leased Prepared Task snapshot, Frontend-owned composer, and Native Session. Returning renders cached state immediately and does not call `task/acquire` or `task/open` merely to prove that the Task still exists. A reconnect or revision gap installs a replacement client-scoped baseline according to [ADR-0023](adr/0023-task-state-publication-and-replica-recovery.md).

Changing Project, Agent, or Task Workspace calls typed `task/release` and waits for acknowledgement before acquiring another key. Release clears only the authoritative lease; it never clears the Frontend composer. Releasing without a current lease is an idempotent no-op. App Server alone decides whether the released Prepared Task is retained or disposed. The old public `task/discard` operation is removed rather than given release semantics.

## Send

### Request shape and replay rule

`task/send` contains Task identity plus User message content:

```text
taskId
message.text
message.images[] { label, mimeType, data }
```

Each Image is encoded inline from the Frontend-owned draft only when Send is invoked. The request does not resend Project, Agent, configuration values or catalogs, or the slash-command catalog. Those already belong to the Task and Native Session.

Frontend issues each `task/send` mutation once and never automatically replays it after timeout, disconnect, reconnect, reload, or an unknown transport outcome. An ordinary `clientRequestId` may correlate the request and response and record accepted-message provenance, but it is not a retry instruction and is not persisted across reload for replay. Frontend retries only the read-only event stream as defined by [ADR-0023](adr/0023-task-state-publication-and-replica-recovery.md).

### First Send

Frontend marks the acquired Prepared Task as submitting while retaining the Composer draft until the authoritative result is known. Invoking New Task while Send is pending reopens that same cached submitting instance. If cache loss requires App Server resolution, acquiring the same key returns that instance while it remains leased and private. After authoritative acceptance makes the Task visible, a later New Task action acquires another Prepared Task. Rejection keeps the same lease and recoverable draft. Transport loss enters connection-lost presentation and resynchronizes without replay. Returning from browser suspension restarts the replayable receive poll from the last applied server sequence, so accepted Task events catch up without requiring a page reload or replaying Send.

The durable first-Send transaction:

1. acquires the per-Task command lock shared by every Send-relevant Task mutation;
2. reads Task state and validates the exact client lease, readiness, message shape, inline Images, aggregate limits, and Agent capability once under that lock;
3. durably appends the User message and sets Task state to `starting`;
4. changes New Task lifecycle from leased Prepared Task to visible, consuming the lease;
5. updates state-root last-used Project and Agent defaults;
6. publishes the visible Task into Task Navigation and normal Task subscriptions;
7. releases the command lock;
8. returns the authoritative Task snapshot containing the accepted User message.

Promotion and message acceptance are atomic for query readers. A stale Send after release or any other failed validation changes no durable Task state and leaves the Frontend draft untouched. App Server performs no ACP I/O while holding the Task command lock.

App Server materializes the acceptance response from durable state before ACP prompt work. The accepted Task is `starting`, because the Agent has not received the prompt. After commit, App Server gives the Task's opaque Native Session handle and accepted message content to `NativeSessionService.startPrompt` in background execution. The service converts each accepted Image to ACP Image content and owns whether the underlying ACP session is live or must be loaded, resumed, or recreated. When prompt execution actually begins, the Task becomes `working`. A definitive service or Agent failure after durable acceptance becomes a Task state transition; it does not retroactively reject the accepted User message or restore the Frontend draft.

Frontend remains on the New Task surface in submitting state until it reconciles acceptance. Because the UI permits only one in-flight Send per Task, success clears that Task's Composer directly and asks the App Shell to route to the now-visible Task id. It does not compare message text, message id, idempotency key, or a settlement key to clear the acknowledged Composer. Rejection leaves the Composer unchanged and does not route to the Task page.

First Send performs no history synchronization because the New Task has no Agent conversation history. Acceptance or scheduling alone never publishes `historySync: syncing`.

### Steering messages

A Send accepted while the Task is `working` is a steering message. App Server durably appends its text and Images to Chat and returns authoritative Task state, then asks `NativeSessionService.steer` to forward that accepted content to the same Native Session as another `session/prompt` request. The workflow does not wait for the steering response, and that response never controls Task status. The transport consumes or safely discards any eventual JSON-RPC response.

## Native Session Updates And Chat

### Update-consumer lifetime

One Native Session update consumer exists from acquisition until session close or replacement. It processes every `session/update` in arrival order regardless of which prompt is active or has returned. Prompt completion never finalizes, detaches, or narrows this consumer.

Agent message chunks with the same Agent-owned `messageId` update the same in-progress Chat message; interleaved message ids remain separate. Tool updates use `toolCallId` and never depend on an OpenAIDE Turn id.

### Live Agent and Thought text

Smooth streaming is Frontend-only ephemeral presentation layered over immediately updated authoritative Chat. The authoritative Task reducer owns one Chat replica, not a second presented-Chat array. Only the selected Agent or Thought row owns an ephemeral visible-text cursor. Initial open, Task switch, a baseline, a hidden browser tab, and reduced-motion preference render all known text immediately without an animation backlog.

Only live Chat append or chunk events received while the Task is open may advance a presenter. Agent text animates only for the latest Agent text message; Thought text independently animates only for the latest Thought message. “Latest” within a channel does not mean the final row in the mixed timeline. When a newer message appears in one channel, Frontend flushes the previous message in that channel and moves its presenter and caret to the newer message. Background Task events update authoritative cached state without animation.

Animation never gates, hides, delays, or reorders later Chat rows. Frame-driven presenters catch up within 96 ms of the newest update regardless of chunk size. Only the selected latest message in each channel shows its caret. The live-event signal and authoritative Task change enter the reducer as one action so one server event causes one root state transition.

While an active turn has a live status footer, Frontend shows its elapsed wall time from the App Server-authored running-turn timestamp. The timer appears after five seconds, updates locally once per second, and never changes authoritative Task state or rerenders unchanged Chat rows. It remains a single trailing item separated from the truncating status label by a quiet vertical hairline. Completed and inactive turns show no live timer.

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

Frontend identifies each uninterrupted run of Tool and Thought rows. One Thought remains a standalone `Thinking` disclosure. Every other run becomes one activity disclosure, including a single Tool, multiple Thoughts, or any Tool/Thought combination. Adjacent groupable rows extend the group while preserving its open state. User, Agent, Permission, Question, and Live Activity rows end the group. The group preserves every underlying message id and chronological order.

Expanded mixed Tool/Thought groups may hide Thought rows by default and expose one leading control with the hidden count. Showing reasoning restores Thoughts at their original positions among Tools. Thought-only groups always show every Thought when expanded, regardless of the mixed-group visibility default. This presentation does not affect `think` Tool calls.

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

### Idle Native Session release

App Server releases an inactive Native Session after 30 minutes without session activity when the Agent advertises ACP `sessionCapabilities.close`. Activity is measured by the Native Session worker, not Task Page visibility or subscription count. Session setup, outbound session operations, and inbound `session/update` notifications restart the deadline. A prompt, Configuration Option mutation, permission, question, or other in-flight session operation suspends expiration until that operation settles.

Expiration sends `session/close`, ends the local worker, and releases its live resources without changing Task status, Chat, cached Configuration Options, or the durable Native Session id. The next Send or Configuration Option mutation restores the binding through `session/resume`, falling back to `session/load` when resume is unsupported, and reattaches the Task's permanent update sink. Recovery never replays an accepted prompt.

Agents that do not advertise `sessionCapabilities.close` are not idle-expired because ACP forbids Clients from sending `session/close`; those sessions retain process-lifetime cleanup. Ordinary navigation never closes a Native Session directly.

## Task Attention And Web Notifications

App Server owns one latest outstanding Task Attention Event for each Task. The event has a stable identity, reason, and occurrence time and is included in authoritative Task snapshots and ordered Task changes. It is distinct from generic Task status and `unread`: clients never infer notification-worthy work from a `waiting`, idle, failed, or unread transition alone.

App Server creates or selects the current event using these product reasons:

- `finished` when a primary prompt ends normally;
- `needsPermission` while at least one permission request needs a response;
- `needsAnswer` while at least one Agent Question needs a response;
- `stopped` when the Agent cannot continue because of a token or request limit, refusal, or another non-user stop reason;
- `failed` when active work ends through an unexpected runtime, protocol, connection, cancellation, restart, or recovery failure.

A user-initiated Stop creates no Task Attention Event. When several transient requests are pending, the current event represents the newest outstanding request; resolving it selects the next still-outstanding request without changing that request's stable event identity. Resolving the final request clears the waiting event immediately, even when another client responded. A later finish, stop, or failure creates a new event. Explicit Task-attention acknowledgement clears the current event and `unread`; passive rendering or background visibility does not acknowledge it.

The Web App may present a Task Attention Event as an OS notification only while at least one Web App page remains open. Closed-app delivery, background App Server lifetime, Web Push, and installed-PWA delivery are outside this specification. All visible Tasks in the connected state root qualify regardless of Project or which client started the turn.

Notification enablement is local to one browser profile and origin. Common Settings exposes one Desktop notifications control for all reasons and requests browser permission only from that explicit user action. The control distinguishes off, enabled, browser- or OS-blocked, and unsupported states; it never presents a synchronized App Server preference as proof of local permission. Notification sound follows browser and OS defaults.

Tabs for the same browser profile coordinate attention and delivery. The Web App is unattended only when none of its tabs has focus. Notification eligibility is decided at the event occurrence time: an event that occurs while any tab is focused remains an in-product indicator and does not become eligible merely because the user leaves later. A reconnect may deliver a missed event only when notifications were already enabled at occurrence, the profile was unattended, the event remains current, and the profile has no delivery receipt for its stable identity. Initial startup never turns an existing unread backlog into OS notifications.

The browser profile presents at most one current OS notification per Task, replacing an older notification for that Task. It shows the Task title and the short product reason only; Chat, Agent response, Tool, permission, and question content remain private. Clicking anywhere on the notification focuses or opens OpenAIDE and routes directly to the Task. The notification closes when clicked, when that Task is actively acknowledged, when its waiting request is no longer current, or when a newer event replaces it. Merely focusing another OpenAIDE Task does not clear it.

App Server owns Task Attention meaning, identity, persistence, ordering, and clearing through product mutations. The Web App shell owns local opt-in, browser permission, focus observation, cross-tab coordination, delivery receipts, OS notification presentation, replacement, closing, and Task routing. Shared Frontend may carry the authoritative event to the shell through a narrow presentation seam, but it does not infer attention policy or call browser notification APIs directly. The existing client-scoped `shell/showNotification` request remains available for explicit App Server-to-shell messages and is not the Task Attention lifecycle.

### ACP update scope

- ACP `plan` updates are ignored until Plan presentation and persistence are deliberately specified.
- OpenAIDE uses Session Config Options as its single product model. Dedicated Session Modes are ignored with diagnostics rather than synchronized into a second Mode model.
- ACP `usage_update` is ignored with diagnostics until context-window and Agent-reported cost presentation are specified.

## History Synchronization

Opening an existing Task is the only automatic Native Session recovery and history-synchronization trigger. App Server returns stored Task state immediately, then uses `session/resume` when Chat is not proven stale, falls back to `session/load` when resume is unsupported, and calls `session/load` directly when the cached Agent timestamp proves Chat is stale. Opening does not issue `session/list`; neither Send nor catalog refresh initiates history synchronization. Exact catalog, tolerance, replacement, failure, and publication behavior is defined by [ADR-0023](adr/0023-task-state-publication-and-replica-recovery.md), while timestamp persistence is defined by [ADR-0024](adr/0024-task-chat-persistence.md).

When synchronization is required, Frontend keeps the stored Chat visible, shows `historySync: syncing`, and disables Send. A successful `session/load` atomically replaces Chat with exactly the rendered replay. Failure keeps existing Chat, appends `History update failed` Live Activity, ends syncing, and enables Send.

## Task Titles

A Task stores one optional title with Prompt, Agent, or User provenance. New Tasks have no stored title; Frontend renders `New task` as fallback. First Send stores a normalized 60-character prefix of the first User message as a provisional Prompt title. An Agent title value or clear supersedes Prompt- or Agent-owned state, while User-owned titles remain protected. Native Session adoption stores the supplied session title as Agent-owned. User title mutation remains a future interface.

## Query And Authorization

- Task lists, Task Navigation, Archive, search, ordinary support-facing Task counts, and cross-client subscriptions exclude New Tasks.
- Only the leasing client may receive or subscribe to its acquired Prepared Task. Free Prepared Tasks and the internal pool inventory are never exposed through client snapshots.
- Non-lessee open, Send, configure, reveal, release, and subscribe intents return one stable authorization or not-found error without revealing existence.
- Explicitly internal cleanup and support diagnostics may inspect New Tasks.
- After promotion, normal visible-Task authorization and subscription rules apply.

## Images And Workspace File Mentions

An unsent Image is part of the Frontend-owned Composer draft, not a Task resource. Paste, drag/drop, and the image picker are only input methods for the same Image content kind.

- Frontend retains Image bytes, safe preview URLs, display labels, and row order for the lifetime of the live page.
- Navigation, reconnect, App Server restart, and Project, Agent, Task Workspace, or Prepared-Task lease changes leave the whole Composer draft untouched while that page remains alive.
- Full page reload, tab close, or explicit row removal may release the local bytes and preview. The App Server has no unused resource to clean up.
- On Send, Frontend encodes the current Images inline in `task/send`. App Server validates supported MIME types, encoding, individual and aggregate limits, message shape, and Agent Image capability in the same acceptance transaction.
- If the selected Agent lacks Image capability, Frontend retains the Images and blocks Send with an actionable explanation. App Server repeats the capability validation authoritatively.
- Failed validation or unknown transport outcome leaves the local draft intact. Durable acceptance stores the Images with the User message; later ACP delivery failure is Task failure and does not restore or consume a second copy of the draft.
- There are no pre-Send upload calls, attachment handles, Draft resources, Task-scoped image authorization rules, cross-tab handles, or unused server-side image cleanup.
- Arbitrary device-file attachment is a follow-up feature. It must reuse this client-owned unsent-draft boundary and define its ACP representation and limits rather than revive Task-scoped pre-Send uploads.

Workspace files are not attachments. Typing `@` at the start of the prompt or after whitespace opens completion for the current Task Workspace. The App Server searches a bounded, watched index of tracked and non-ignored untracked files using effective Git ignore rules. Selecting a result inserts ordinary text as `@relative/path`, or `@"relative/path with spaces"` when quoting is required. The text remains with the Frontend draft across Agent, Project, and prepared-Task changes.

The composer and persisted User messages style this syntax without adding click behavior or claiming the path still exists. `task/send` and ACP `session/prompt` receive unchanged text: this slice creates no attachment handle, structured mention, ACP `resource_link`, or embedded `resource`. The add-context menu offers image input only; workspace-file selection is exclusively the `@` completion flow.

## Options And Slash Commands

Agent-provided catalogs are authoritative Task projections. Frontend caches them for rendering and sends dedicated option-change intents; `task/send` contains neither catalogs nor selected option values. Catalog publication follows [ADR-0023](adr/0023-task-state-publication-and-replica-recovery.md).

Option changes follow one ordering model:

1. Frontend marks the changed control pending without publishing the requested value as authoritative.
2. App Server serializes user option requests for one Native Session.
3. Agent responses and independent Agent notifications enter one monotonically ordered Task revision stream.
4. Frontend applies the newest complete catalog and ignores stale response state.
5. A user request superseded by newer Agent state resolves to the newest catalog without a race-only user error.
6. Visible errors are reserved for genuine transport, setup, authorization, unsupported-operation, or Agent failures.

While an option mutation is pending, Frontend renders the requested value in that control with a busy indicator and locks every configuration selector. The existing Task's Agent remains locked, while drafting and Image actions remain usable. If the mutation is still pending after five seconds, Frontend adds the quiet status text `Agent is still updating options…` without replacing the Composer or reporting an error.

App Server allows up to 60 seconds for the Agent to answer the option request. A failed or timed-out mutation clears pending state, restoring the last Agent-confirmed catalog, and Frontend presents the mutation error. That error clears after ten seconds or earlier when a later complete Agent catalog changes. A late catalog that confirms the requested value renders normally through the same authoritative catalog path.

Slash-command catalogs use the same snapshot and event ordering. Slash commands and `@file` completion provide Composer assistance and add no structured state to `task/send` or Chat.

## Conformance Invariants

An implementation conforms to this specification only when all of these are true:

1. One stable client leases at most one Prepared Task, one Prepared Task is leased to at most one client, and only a ready matching free entry is reused.
2. First-Send message acceptance, lease consumption, and Prepared-Task promotion are one durable atomic mutation.
3. Each Send mutation is issued once and is never automatically replayed.
4. One Native Session update consumer survives prompt completion and accepts later updates until session close or replacement.
5. One durable Task transaction produces one ordered Task revision; a revision gap installs one new baseline.
6. Connection recovery retries only the event stream, then installs exactly one baseline for each active scope before Send is enabled.
7. Durable Chat, transient requests, Tool details, and Frontend-only presentation each have one explicit owner and do not masquerade as one another.
8. Every notification-worthy Task transition creates one explicit Task Attention Event; no client infers it from status or `unread`.
9. A browser profile emits at most one OS notification for one eligible Task Attention Event and never emits an old unread backlog on startup.
10. App Server owns Task Attention state while each App Shell owns its local attention and notification capabilities.
11. Unsent Composer text and Images have one Frontend owner and remain unchanged by Project, Agent, Task Workspace, navigation, reconnect, or Prepared-Task lease changes while the page remains alive.
