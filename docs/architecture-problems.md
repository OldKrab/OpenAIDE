# Architecture Problems

This is the living register of architectural and encapsulation problems found while tracing OpenAIDE behavior. It records confirmed problems before solution design. Fixes should be discussed and handled one at a time. The audit's primary goal is simplification: fewer owners, states, policies, ordering mechanisms, recovery paths, and caller obligations produce a smaller bug surface. Complexity is retained only for a named product invariant or demonstrated common failure.

## Status

- `confirmed`: evidence shows a real design problem.
- `investigating`: the concern is plausible, but the surrounding flow still needs tracing.
- `resolved`: the agreed fix is implemented and verified.

## AP-001: Stale asynchronous result protection is fragmented

**Status:** confirmed

**Area:** Frontend navigation and asynchronous workflows

The Frontend uses several overlapping identities to prevent obsolete asynchronous results from changing current state:

- `navigationGeneration` in `packages/frontend/src/components/appControllerBackendLifecycle.ts`;
- the independent generation and request ordering in `packages/frontend/src/state/snapshotRequests.ts`;
- operation-local generation comparisons in New Task preparation, file browsing, Native Session adoption, and configuration mutations;
- additional preparation keys, request ids, and App Server replica epochs.

These mechanisms do not all duplicate the same behavior: snapshot request ordering is more specific than navigation invalidation. The problem is that callers must understand which guards to create and combine. Correct stale-result handling is therefore distributed across workflows instead of encapsulated behind one clear interface.

In the Web App, an in-app navigation intent can also invalidate the generation once before posting the route request and again when the resulting browser route event is consumed. This is currently harmless, but it indicates unclear ownership of navigation invalidation.

**Impact:** A new asynchronous path can omit one guard, accept a late response, or apply a result to the wrong Task, route, preparation context, or App Server replica.

**Desired direction:** Put operation currency and result acceptance behind a deeper navigation/async-operation module. Preserve specialized ordering where needed, but remove the need for individual workflows to coordinate several independent stale-response mechanisms.

## AP-002: Shared Frontend owns concrete App Shell routing

**Status:** confirmed

**Area:** Frontend and App Shell seam

`packages/frontend/src/services/hostBridge.ts` is inside the shared Frontend but selects concrete shell behavior at runtime. It knows about VS Code webview messaging, browser history, Web App URL shapes such as `/new-task` and `/task/:id`, route parsing, and standalone development behavior.

The architectural intent is sound: shared product UI should request navigation without knowing how a shell presents it. The current seam is misplaced because the shared package also implements shell selection and Web-specific routing. Its broad `postHostMessage` interface mixes navigation with secrets, diagnostics, telemetry, workspace operations, and other shell capabilities.

**Impact:** Shared Frontend code is coupled to current shells and transports. Adding or changing an App Shell can require edits inside the shared Frontend, while tests must account for runtime shell detection and unrelated message families.

**Desired direction:** Define narrow shell interfaces at composition seams. Keep browser routes and history in the Web App adapter, VS Code panels and webview messaging in the VS Code adapter, and inject the selected adapters into the shared Frontend.

## AP-003: Task title state is split and uses display text as a lifecycle marker

**Status:** resolved

**Area:** Task domain model and presentation

`TaskRecord` stores both a local `title` and an optional `agent_title`, then computes an `effective_title` by giving the Agent value priority. A New Task persists the display text `"New task"` as its local title. First-send logic detects that lifecycle state with `if task.title == "New task"` before deriving a replacement from the prompt.

This mixes presentation fallback, lifecycle state, and title ownership. It also leaves callers to understand two title fields and an implicit precedence rule. Changing or translating the visible placeholder can affect domain behavior.

**Impact:** Title changes can overwrite the wrong owner, lose provenance, or accidentally depend on UI wording. The persisted record does not directly express why the current displayed title exists.

**Desired direction:** Store one optional current Task title together with explicit User or Agent provenance. Keep `"New task"` as a render-only fallback when no title exists. Do not derive a title from the first prompt. Define replacement precedence explicitly instead of encoding it through separate fields or magic strings.

**Implementation:** `TaskRecord` now stores one optional `TaskTitle { value, source }`. New Tasks and first Send leave it absent; `"New task"` and `"Untitled task"` are Frontend-only fallbacks. Agent metadata may replace or clear only an Agent-owned title and cannot overwrite a User-owned title. Native Session adoption records its supplied Agent title as Agent-owned. The protocol exposes the same optional nested value, and prompt-derived title invention, the magic lifecycle string, and Frontend title rewriting were removed without compatibility behavior.

## AP-004: The Frontend App Controller is an orchestration nexus

**Status:** confirmed

**Area:** Shared Frontend composition and workflow ownership

`packages/frontend/src/components/appController.ts` is a 411-line hook with 28 imports. It creates or coordinates route/bootstrap state, App Server connection lifecycle, replica replacement, global reducer state, Agent and preference projections, New Task preparation, prepared-Task ownership, attachment resources, ambiguous send recovery, Native Session loading, Task attention receipts, telemetry, derived navigation state, and all UI callback families. Its backend lifecycle dependency is another 563-line hook that also participates in routing and snapshot acceptance.

The problem is not the line count alone. Many workflows meet through shared refs, generations, reducer actions, and effects in the controller. Understanding one user action requires moving between the controller, backend lifecycle, routing hook, callback assembly, workflow-specific callback modules, and reducers. The controller's returned interface also exposes both derived state and low-level dispatch alongside grouped callbacks.

**Impact:** Workflow ownership and ordering are difficult to see locally. Changes can create hidden effect interactions, stale closure or ref dependencies, duplicate invalidation, and broad tests that must construct most of the application controller to verify one behavior.

**Desired direction:** Keep a small Frontend composition root and move complete workflows behind deep, typed interfaces. Separate shell routing, App Server session/replica lifecycle, Task workspace state, New Task lifecycle, and send recovery so each owns its invariants and exposes user-intent operations plus render-ready state. Avoid a replacement catch-all controller split only by file; seams should follow behavior and ownership.

## AP-005: Navigation discards New Tasks and their composer resources

**Status:** resolved

**Area:** New Task lifecycle and Frontend navigation

Leaving the New Task route triggers `PreparedTaskOwnership.discard` from `useNewTaskPreparation`. Opening Settings or an existing Task also calls the same discard workflow from `navigationCallbacks`. That workflow clears `taskInputs[taskId]`, removes the Task locally, releases its attachment resources, and sends `task/discard` to App Server.

This makes view navigation an implicit destructive Task-lifecycle operation. It contradicts the product rule that leaving the New Task surface does not discard the New Task or close its Native Session, and it prevents the Frontend from simply retaining the known Task id and local composer state for return navigation.

**Impact:** Switching surfaces can lose unsent text and attachment selections, repeat Task creation and Agent preparation, create timing-sensitive cleanup/recreation races, and make browser history behave differently from stable Task navigation.

**Desired direction:** Create a client-private New Task once for its required start context and retain its Task id, snapshot, local composer state, and live attachment ownership while the Frontend client remains active. Navigation should hide or suspend the view without discarding product state. Returning should render cached state immediately and refresh/open the known Task id when needed; it should not call `task/create` again. Discard must require explicit user intent or a separately defined lifecycle policy, not ordinary navigation.

**Implementation:** `NewTaskController` owns the client-private New Task identity, snapshot, preparation lease, and Send protection independently from the visible Task reducer. Protocol lifecycle is preserved through Frontend mapping, and New Task snapshots never enter normal Task collections, active Task state, or visible Task caches. Ordinary navigation retains composer input and attachment handles, returning renders the cached New Task without another create/open request, late creation stays in the controller without changing the current route, and a dedicated hidden Task subscription keeps preparation, options, and commands current. Explicit context replacement and discard remain separate product operations.

## AP-006: New Tasks are globally reusable and visible as normal Tasks

**Status:** resolved

**Area:** New Task ownership, storage, and queries

`task/create` currently searches all stored Tasks for an empty reusable record using Agent and workspace fields, without client ownership. Normal storage list queries exclude only tombstoned or archive-mismatched records, so a pre-history New Task can appear in Task Navigation and can be reused across clients.

The agreed model makes a New Task private to the App Shell client that created it. It is retained across that client's navigation and reconnects, but it is not a visible Task and must not be returned by normal Task list, Archive, history, or cross-client query surfaces. The first durably accepted user message promotes the same identity into a visible Task.

**Impact:** One client can observe or reuse another client's pre-history Task, empty Tasks can clutter navigation, and ordinary Task queries expose state that should exist only for the owning client's New Task workflow.

**Desired direction:** Persist explicit New Task ownership by stable `clientInstanceId`, resolve requests through the initialized connection context, and exclude New Tasks from all normal Task collection queries. Provide an owning-client New Task resolve/ensure interface. Atomically clear private ownership and publish the Task to normal collections when the first message is durably accepted.

**Implementation:** `TaskLifecycle::New { owner_client_instance_id } | Visible` replaces `first_prompt_sent`. Creation resolves or creates one New Task per client atomically under the Task mutation lock; client Task intents use centralized non-disclosing ownership checks; normal collections, events, and revisions exclude New Tasks; first Send is the sole New-to-Visible promotion. No legacy deserialization or migration is provided.

## AP-007: Agent configuration races can surface as user failures

**Status:** investigating

**Area:** Agent configuration options and concurrent session metadata

User option changes are App Server requests that call the Agent and later commit the returned catalog, while the Agent may independently emit a new configuration catalog for the same Native Session. The current workflow has mutation tokens and per-Task request serialization, but it can still return conflict errors when the Task or session changes while Agent I/O is in flight. Frontend maps failures to a generic `Unable to update Agent option` error.

**Risk:** A benign race between an Agent-owned catalog update and a valid user selection may be presented as if the user made an invalid action. A late response may also compete with newer Agent-owned state unless one ordering authority covers both response and notification paths.

**Desired direction:** Keep the Agent catalog authoritative, apply only monotonically ordered App Server revisions, and treat a superseded user selection as a non-error reconciliation to the newest catalog. Serialize only what is necessary, avoid optimistic option values, and reserve visible errors for genuine transport, setup, or Agent failures.

## AP-008: New Task defaults are conflated with collection fallbacks

**Status:** resolved

**Area:** New Task Project and Agent selection

`ProjectCollectionSnapshot.activeProjectId` is currently `None` in the production Project collection source, so Frontend selection falls back to the first Project unless a shell route requested one. `AgentCollectionSnapshot.defaultAgentId` is not a remembered user preference; App Server computes it by choosing Codex when present and otherwise the first Agent. Frontend preserves its current in-memory Agent only while that Agent remains in the collection.

This conflates three different concepts: a client-local last-used selection, an optional user-configured global default for a genuinely new client, and a deterministic fallback when neither exists.

**Impact:** The UI can present a heuristic as a user default, lose the client's last choice across reloads, and duplicate fallback policy between App Server collection ordering and Frontend selection logic.

**Desired direction:** Let each live client remember its last-used Project and Agent and reuse them when they remain valid. Persist state-root-wide last-used Project and Agent values in App Server only as initial defaults for clients without retained selection, updating them as part of an already-required successful first send rather than through extra selection traffic. When neither client selection nor valid persisted default exists, apply one documented deterministic fallback after considering the shell Project hint.

The accepted implementation design for `AP-003` and `AP-005` through `AP-008` is recorded in `docs/new-task-flow-plan.md`.

**Resolution:** Project and Agent collection snapshots now contain only collections. `client/initialize` returns a separate state-root-wide `newTaskDefaults` value, while Frontend retains each client's selection locally and validates it through one priority function. A successful first Send from a New Task persists that Task's actual Project and Agent as the next-client defaults; selector changes create no App Server preference traffic.

## AP-009: Composer sendability is duplicated and New Task ignores authoritative readiness

**Status:** resolved

**Area:** Composer capability projection and message validation

`NewTaskView` computes `canSend` from Project/workspace loading, local submission state, option errors, and attachment-handle presence, but does not require `snapshot.send_capability.state == ready`. Existing `TaskView` uses a separate `taskComposerAvailability` path that considers connection, archive, pending/uncertain send, preparation, Task status, and App Server send capability. `Composer` then adds another layer through `submitDisabled`, `submitRequiresText`, attachment count, and local text checks.

The `attachment_only` protocol field is also currently derived only from Task preparation/status: it is false while loading or failed and true for every prepared Task status. It does not prove an Agent-specific attachment-only message capability. A completely empty message is always locally blocked; the field means only that valid attachments may substitute for text.

**Impact:** New Task can present Send as available while authoritative preparation is loading, blocked, or failed; New Task and existing Task can disagree for the same capability state; and boolean prop combinations can represent contradictory composer behavior.

**Desired direction:** App Server supplies one authoritative Task send-capability projection with explicit readiness, structured blockers, and message-shape capability derived from real Task/Agent support. A shared Frontend composer-availability module combines that projection only with truly local facts such as text emptiness, visible attachment rows, and in-flight UI state. Pass a cohesive render model to `Composer` instead of several loosely related boolean props, while retaining authoritative validation in `task/send`.

**Resolution:** App Server now projects only authoritative readiness and structured blockers. Frontend resolves that state with connection, context, submission, text, and attachment-handle facts through one `composerAvailability` model used by New Task, existing Task, and `Composer`. The false `attachment_only` capability and the split boolean Composer API were removed. ACP defines prompts as content-block arrays and exposes capabilities for block types, not a text-required mode, so OpenAIDE allows attachment-only messages when every selected attachment has a valid App Server handle and still rejects a completely empty message.

## AP-010: Send retry recovery is over-specialized and duplicates request identity

**Status:** resolved

**Area:** App Server Protocol mutation identity and Send recovery

The request envelope already supports `clientRequestId`, while `task/send` additionally requires a `TaskSendIdempotencyKey` inside its params. Frontend persists exact pending sends in browser `sessionStorage`, automatically replays them after reload/reconnect, and coordinates Send-specific receipt, fingerprint, reducer, in-flight, and ownership plumbing.

**Impact:** One user intent has two request identities with different typing and persistence rules, and a rare lost-response case dominates Frontend, protocol, storage, and recovery complexity. Automatic replay also performs a product mutation after reconnection instead of first resynchronizing authoritative state.

**Desired direction:** Issue `task/send` once and never automatically replay it after transport loss, reconnect, or reload. Remove the Send-specific idempotency field and durable browser recovery machinery. Reconnect/resubscribe to authoritative Task state: accepted work appears from App Server; rejected work keeps the live in-memory draft; a full reload may lose an unaccepted memory-only draft as an explicit trade-off. Keep ordinary request correlation only where useful.

**Resolution:** `task/send` now carries only the Task id, current revision, and message. Frontend issues it once, keeps only the live in-memory submitting draft, restores that draft after any request failure, and never stores or replays a send after reconnect or reload. App Server no longer stores Send receipts, fingerprints messages, or recovers a request by a second identity; ordinary request-envelope correlation remains transport-only.

## AP-011: Frontend routes to Task before first-send acceptance

**Status:** resolved

**Area:** New Task first-send navigation and durable acceptance

`submitNewTask` starts `executeTaskSendAttempt` and immediately posts `surface.openTask` before awaiting the request result. The App Shell can therefore adopt `/task/:id` while App Server acceptance is still pending, ambiguous, or eventually rejected.

**Impact:** Routing implies that a private New Task has become a visible Task before the authoritative lifecycle transition. Rejected and unknown sends require special rendering on an existing-Task route, shell panel adoption can occur too early, and navigation becomes another participant in send recovery.

**Desired direction:** Keep the New Task surface rendered in submitting state until `task/send` returns durable acceptance. Then ingest the accepted snapshot, clear the submitted composer, and route/adopt the now-visible Task. Rejection keeps the same New Task surface and draft. Transport loss reconnects/resynchronizes without replay.

**Resolution:** New Task now remains routed and submitting while `task/send` is pending. Frontend ingests the accepted visible Task snapshot and settles the composer before asking the App Shell to open the Task. The route title comes only from the accepted App Server snapshot. Rejection restores the draft without routing, and a navigation change while Send is pending prevents the late acceptance from hijacking the current surface.

## AP-012: First Send runs through history and Native Session recovery orchestration

**Status:** resolved

**Area:** First-send background execution and Agent runtime seam

After accepting every Send, App Server publishes `historySync: syncing`, waits for preparation, chooses among Native Session resume/load/new paths, may reconcile replayed history, persists session metadata again, attaches session events, and only then spawns the prompt. This general recovery pipeline also runs for the first message of a prepared New Task, which has no prior Agent conversation history. Send workflow therefore knows Agent runtime recovery details that belong behind the Native Session seam.

**Impact:** First Send carries history generations, replay branches, deferred recovery, duplicated readiness handling, and session-method selection that cannot benefit a history-empty New Task. `syncing` is visible before actual history reconciliation begins, and accepted state reports working before the Agent prompt starts.

**Desired direction:** New Task creation acquires an opaque handle from a deep `NativeSessionService` and establishes its session update subscription once. First Send commits the user message and `starting` Task state, returns acceptance, and starts the primary prompt through the service in background with no history-sync state. The service privately uses an existing, loaded, resumed, or recreated ACP session. Existing-Task history status changes to `syncing` only when real reconciliation starts and to `updated` only when persisted history changes.

History freshness must not compare Native Session activity with generic Task `updated_at` or with the timestamp of the last history synchronization. Generic Task metadata can change without Chat, while Chat can continue changing through live Agent work after a synchronization. Persist a dedicated `localHistoryUpdatedAt` that advances whenever stored Chat changes, and compare the cached Native Session `updatedAt` against that value only when the user opens the Task.

**Resolution:** First Send now commits and returns a `starting` Task without entering history synchronization. A deep `NativeSessionService` owns preparation, start/load/resume/recreate selection, session binding, one update subscription, and primary prompt startup; the Send workflow only hands it the accepted prompt. Task open consults a separately refreshed Native Session catalog and starts `session/load` only when its cached native timestamp is more than five seconds newer than the dedicated durable Chat clock. Successful replay replaces Chat and advances that clock to the native timestamp, while failure records a Live Activity and returns history state to idle. The catalog refreshes independently at process start and every minute, manual session listing also feeds it, and the obsolete checking/failed history states and retry method were removed.

## AP-013: Session updates are incorrectly scoped to an active Turn

**Status:** resolved

**Area:** ACP session event projection and steering

ACP `session/update` notifications identify their Native Session, not the `session/prompt` request that caused them. Message chunks may additionally carry an Agent-owned `messageId`, and tools carry `toolCallId`, but neither is an OpenAIDE Turn id. Current `TaskEventSink` is created for one Turn and rejects writes after `active_turn_id` changes or its Turn cancellation is set. A valid later update can therefore be silently discarded when the primary prompt response has already completed.

**Impact:** Prompt completion controls the lifetime of a session-wide event consumer. This is incompatible with late Agent updates and with steering prompts whose updates cannot be attributed to one prompt request. It also forces message streaming, tools, permissions, options, and commands through unnecessary Turn identity checks.

**Desired direction:** Install one update consumer for the lifetime of the acquired Native Session and keep it attached until the session closes or is replaced. Persist every accepted session update in arrival order. Use Agent `messageId` to append later chunks to an in-progress Agent message and `toolCallId` to update tools. Do not require an active OpenAIDE Turn for a session update to reach Chat. The primary prompt response controls Task `working` state only; it does not end the update subscription.

**Resolution:** Each opened ACP Native Session now owns one permanent update projection and one Task-bound session sink. The Native Session service retains and reuses that sink for prompt execution, so text, thought, tool, command, option, and metadata updates pass through one ordered session boundary and are accepted while the same Native Session remains bound, even after the primary prompt response clears the active Task work. Prompt completion no longer drains an arbitrary timing window or finalizes session-owned streaming runs; the prompt sink remains only for prompt-specific requests such as permission handling.

## AP-014: Non-text Agent message content is silently discarded

**Status:** resolved

**Area:** ACP live update projection

ACP `agent_message_chunk` carries a general `ContentBlock`, which can contain text, images, audio, embedded resources, or resource links. `LivePromptProjection::emit` currently forwards the update only when its content is text; every other valid content block falls through without persistence, an App Server event, or a visible unsupported-content indication.

**Impact:** Valid Agent output can disappear silently, leaving saved Chat and every connected client incomplete.

**Desired direction:** Normalize supported ACP content blocks into typed App Server Chat parts at the permanent Native Session boundary. Preserve unsupported-but-valid output as an explicit safe Chat representation rather than silently dropping it. Frontend renders only the normalized App Server Protocol model and does not interpret raw ACP payloads.

**Resolution:** Live and replayed ACP Agent message and Thought updates now normalize all five ACP content kinds at the Native Session boundary. Text retains its incremental update path; valid bounded images, embedded text resources, and resource links persist as typed App Server Chat content and project through dedicated protocol parts. Audio, binary resources, and malformed images persist as explicit unsupported-content rows with safe metadata and diagnostic logging instead of disappearing. The shared Frontend renders image previews, compact resource disclosures, and clear unsupported-content status without receiving raw ACP objects or reserved metadata.

## AP-015: Agent message completion and identity depend on prompt-scoped memory

**Status:** resolved

**Area:** streamed Agent message persistence and presentation

For live output, App Server keeps an in-memory `StreamingRuns` map from optional Agent `messageId` to a randomly generated local Chat id. It marks messages complete by calling `TaskEventSink::finish` when the prompt-scoped sink finishes. This loses the mapping at prompt completion even though later session updates may still target the same Agent message, and it makes completion depend on the prompt lifecycle rather than the session update model.

**Impact:** Late chunks can be dropped by the active-Turn guard or become a second Chat row. Live and replayed copies of the same Agent message can receive different local identities. The implementation also invents internal final chunks even though ACP does not provide a final-message-chunk marker.

**Desired direction:** Derive or persist stable Chat identity from Native Session id plus Agent `messageId` for the lifetime of the session, so any later chunk updates the same row without prompt-owned lookup state. Treat smooth reveal and carets as ephemeral Frontend presentation for the latest Agent text message and latest Thought message in the currently opened Task. Other timeline rows do not supersede them; only a newer message in the same channel does. Never animate multiple messages in one channel, persist a fictional ACP message-final event, or let animation hide/delay other rows. Keep a small explicit fallback only for ACP v1 chunks that omit `messageId`.

**Resolution:** Sourced live and replayed text now derives one Chat identity directly from Native Session id plus ACP `messageId`; late chunks append atomically to that row without prompt-owned correlation state. Anonymous ACP v1 chunks use only one small current-run slot per Agent/Thought channel and split at explicit content boundaries. Persisted messages are always complete: the protocol no longer invents chunk sequence, final-chunk, or stored streaming fields, and the Task event cursor remains the only delivery ordering mechanism. Frontend receives a live-presentation signal only for post-baseline text events and locally reveals the latest Agent and latest Thought message in the mounted Task. Baseline, replayed, missed, older, and non-text content appears immediately. Thought identity inside grouped Tool presentation remains owned by AP-017.

## AP-016: Tool updates republish broad state and use lossy suppression

**Status:** confirmed

**Area:** ACP tool projection, App Server publication, and Frontend detail refresh

A tool mutation currently persists an upserted Activity row without a committed Chat delta. App Server consequently publishes a complete Task snapshot plus Task summary, Project collection, and Task Navigation updates. `LivePromptProjection` suppresses similar pending/running tool updates inside a 250 ms window to reduce that cost, but it has no scheduled trailing flush; the newest detail can remain only in memory when no later update arrives. Frontend separately polls an expanded running tool's artifact every 250 ms, which cannot retrieve an update that App Server never persisted.

**Impact:** A local tool detail change causes broad state and network churn, while the attempted optimization can lose the newest persisted/displayed output. Polling duplicates the Agent update stream and scales with expanded clients.

**Desired direction:** Persist every accepted tool update. Publish only a typed lightweight Chat-row upsert for shared summary/status changes. Keep full detail behind a per-client subscription created on expansion and removed on collapse; App Server pushes detail changes only to subscribed clients while retaining the latest detail for later reads. Remove Frontend polling and the lossy suppression. Add coalescing only after measurement and require a guaranteed trailing flush.

## AP-017: Tool projection and presentation are incomplete

**Status:** confirmed

**Area:** ACP tool normalization and Frontend Activity UI

OpenAIDE recognizes every current ACP tool kind, but several defined kinds use a generic icon, generic grouped classification, and generic detail renderer. Valid ACP content is also projected lossily: image, audio, resource, and resource-link content becomes a label, while arbitrary nested raw input/output is reduced to sanitized selected fields. The working-tree activity grouping also drops underlying Thought identity when folding Thought rows into Tool groups, preventing precise live Thought presentation.

**Impact:** Distinct Agent actions look generic, supported output can disappear, and grouped Thought updates cannot retain stable presentation identity.

**Desired direction:** Give every defined ACP kind except `other` a distinct appropriate icon, action label, grouped classification, and field-driven detail view. Represent `think` as a Thought-like tool step without confusing it with `agent_thought_chunk`. Normalize supported ACP content into typed safe detail parts and render unsupported valid content explicitly rather than silently discarding it. Preserve underlying Tool and Thought ids in every presentation group.

## AP-018: Pending permissions are duplicated as workflow state and Chat history

**Status:** confirmed

**Area:** ACP permission projection, App Server requests, and Frontend Chat presentation

App Server currently persists a pending Permission Chat row while also delivering a transient server request. Frontend then reconciles multiple representations and identities for one permission, including deduplication, replacement, and timeline repositioning.

**Impact:** A request that exists only while the Agent is waiting becomes false durable history, and Frontend needs complicated synchronization to decide which copy to show. Reload and multi-client resolution behavior become harder to reason about.

**Desired direction:** Keep a pending permission only as an active App Server request and set Task status to `waiting`. Redeliver that active request to reconnecting eligible clients and render it transiently after current Chat while session updates continue. After a user response or prompt cancellation, close it for all clients and persist exactly one resolved Permission Chat item beside its associated activity. Represent cancellation through the same resolution path with its own message.

The same problem and desired lifecycle apply to ACP form elicitation Questions, except that a resolved Question remains a standalone Chat row. Frontend field validation provides immediate feedback; App Server independently validates the response as the authoritative protocol boundary.

## AP-019: ACP Plan updates are discarded

**Status:** confirmed; deferred

**Area:** Native Session update projection

`AcpLivePromptProjection` does not handle `SessionUpdate::Plan`, so complete Agent Plan updates fall through the default branch and disappear.

**Desired direction:** Design Plan persistence and presentation separately in the future. Plan support is explicitly outside the current simplification refactor; do not introduce a partial representation as incidental work.

## AP-020: Some ACP session updates are silently ignored

**Status:** confirmed; partially intentional and deferred

**Area:** Native Session update projection and replay

The live projection uses a catch-all branch for unhandled updates. This hides both intentional behavior and missing support: live `user_message_chunk` echoes are ignored to avoid duplicating App Server-owned user messages, while deprecated `current_mode_update`, deferred `plan`, and unstable `usage_update` also disappear without diagnostics. Replay uses text user-message chunks but discards supported non-text content.

**Desired direction:** Replace the catch-all behavior with explicit handling or diagnostics for every known ACP update. Keep live user-message echoes intentionally ignored and documented while using them during history replay. Defer legacy Session Modes, Plan, and Usage UI as recorded in the implementation plan. Preserve supported non-text content when the broader ACP content projection is implemented.

## AP-021: Prompt stop reasons are discarded and completion uses a drain timer

**Status:** confirmed

**Area:** primary ACP prompt completion

`ActivePrompt` traces the `session/prompt` response but converts every valid response to `Ok(())`, discarding `stopReason`. Task completion therefore cannot distinguish `end_turn`, token/request limits, refusal, or Agent-confirmed cancellation. The runner also waits a fixed 100 ms after the response in an attempt to collect preceding updates.

**Impact:** Meaningful Agent outcomes disappear from the UI, and correctness depends on an arbitrary timing window even though session updates are not owned by prompt completion.

**Desired direction:** Preserve and interpret the ACP stop reason. Let the primary response control only Task `working` state, show non-normal outcomes as Live Activity where appropriate, and keep cancellation single-sourced. Remove the drain timer once the permanent Native Session listener owns all later updates.

## AP-022: Cancellation reports completion before the Agent is cancelled

**Status:** confirmed

**Area:** Task Stop, ACP cancellation, and activity completion

The current `task/cancel` transaction marks running activities completed, persists `Task was stopped`, changes the Task to inactive, and clears `active_turn_id` before signalling the Agent worker to send `session/cancel`. Late updates can then be rejected by Turn guards, and the eventual ACP `cancelled` response carries no product meaning.

**Impact:** The UI reports a completed cancellation before it has started at the protocol boundary, interrupted Tools look successful, and valid late updates can disappear.

**Desired direction:** Replace the old path completely. Introduce `stopping`, cancel transient requests through their shared resolution path, send ACP cancellation, continue consuming updates, and become idle only when the primary prompt confirms cancellation. Mark unfinished activity interrupted rather than completed; surface definitive cancellation failure with explicit recovery.

## AP-023: Active-work termination is split across incompatible cleanup paths

**Status:** confirmed

**Area:** Agent failure, session-worker exit, App Server restart, and user cancellation

Prompt errors, Stop, and restart currently use separate Turn-based transitions with different status and activity behavior. An idle opened-session worker can exit without directly notifying Task state, while restart recovery marks interrupted activity completed and invalidates catalogs through another path.

**Impact:** The same domain event—active Agent work ending without normal completion—produces inconsistent Chat, Tool, request, and Task state. Some session loss is invisible and some interrupted work is reported as successful.

**Desired direction:** Use one cause-aware termination pipeline for Stop, Agent loss during starting/working/waiting, and restart. Share request closure, Tool interruption, idle transition, and one Live Activity; vary only protocol actions and user-facing cause. Idle handle loss remains quiet and is recovered only on the next explicit Send. Never automatically retry a prompt.

## AP-024: Ordinary Task mutations fall back to broad snapshot publication

**Status:** confirmed

**Area:** App Server state subscriptions and Frontend replicas

Only Chat append/chunk and history state currently have committed Task deltas. Any other Task mutation falls back to publishing Task summary, complete Task snapshot, complete Project collection, and complete Navigation. A broad client cursor also forces each scoped subscription to consume and classify unrelated events.

**Impact:** Small local changes create unnecessary reads, serialization, traffic, reducer work, and cursor-gap recovery. One atomic storage mutation is represented as several independently delivered views of state.

**Desired direction:** Replace the fallback completely with one scope-local ordered stream. Initial subscribe/reconnect returns a full baseline; every durable Task transaction emits exactly one focused `taskChanged` event at the next Task revision. Apply its changed fields atomically. On a revision gap, discard the replica and obtain one fresh baseline. Publish Navigation summary only when Navigation-visible state actually changes.

## AP-025: One ACP message can split into colliding Chat rows by content kind

**Status:** confirmed

**Area:** ACP content identity and Chat composition

ACP defines chunks with the same `messageId` as parts of one logical message, but OpenAIDE currently persists text, image, resource, and unsupported blocks as separate Chat rows. Reusing the message-derived identity for those different rows either collides in storage or requires an undocumented content-specific identity, while losing the fact that the blocks belong to one message.

**Impact:** A valid mixed-content Agent or Thought message can overwrite content, split unpredictably, or receive identities that differ between live updates and history replay.

**Desired direction:** Represent one ACP `messageId` as one ordered logical Chat message containing typed parts. Live updates and replay must use the same message identity and part ordering. Do not solve this by inventing unrelated row identities for each content kind.
