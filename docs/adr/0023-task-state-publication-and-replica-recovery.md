# Task State Publication and Replica Recovery

Status: accepted

This ADR defines the ordered publication, history-synchronization, and connection-recovery architecture that supports the accepted [Task Lifecycle and Chat Specification](../task-chat-flow.md).

## Scope-Local Publication

- `state/subscribe` returns one complete authoritative baseline and its scope-local revision. Baselines are used for initial subscription, stream replacement, revision-gap recovery, and explicit complete history replacement.
- Each subscribed scope has an independent ordered stream. A Task scope does not advance through unrelated Project, Agent, Navigation, or other Task events.
- Each durable Task transaction increments one Task revision and publishes exactly one `taskChanged` event. Its payload contains the fields changed atomically by that transaction.
- Focused Task changes include appended or upserted Chat items, text appended to a stable message id, status, lifecycle, title or summary, complete configuration and command catalogs, Send capability, history state, and other accepted Task fields.
- A missing next Task revision invalidates that subscription replica. Frontend obtains one fresh baseline instead of reconstructing or retrying individual events.
- Transient Permission and Question delivery remains outside durable Task revisions until resolution changes a Tool permission outcome or persists a Question Chat item.
- Per-client Tool-detail subscriptions remain separate from shared Task state.
- Complete history replacement is an explicit atomic baseline replacement, not a sequence of synthetic Chat deltas.
- Navigation receives a focused summary change only when a transaction changes Navigation-visible Task state.

## Configuration And Command Catalogs

ACP `config_option_update` and `available_commands_update` each contain a complete current catalog. The Native Session update consumer replaces the corresponding Task catalog and publishes one `taskChanged` event whose focused change is respectively `configOptionsUpdated` or `availableCommandsUpdated`. Frontend replaces its cached catalog and rerenders Composer controls. Catalog changes do not create Chat rows or publish unrelated Project, Agent, Navigation, or complete Task state.

User option changes are serialized per Task. The complete catalog returned by `session/set_config_option`, or a later complete Agent update, is authoritative. An Agent and user race reconciles to the newest catalog without presenting a race-only failure as user error.

Command normalization preserves all supported command-input information rather than reducing commands to only name, description, and an unstructured hint.

## Native Session Catalog

`NativeSessionCatalogService` owns cached `session/list` results keyed by Agent and Project Context. It supplies Native Sessions and their `updatedAt` values to history surfaces. It refreshes active catalogs:

- once per minute;
- on explicit user Refresh;
- after a user prompt successfully starts.

Concurrent background refresh requests coalesce while preserving one trailing refresh. Each result reconciles non-empty Agent titles into matching stored Tasks before owned sessions are filtered from adoption results. Catalog updates never initiate Task history synchronization.

## History Synchronization

Opening an existing Task is the only automatic Native Session recovery and history-synchronization trigger. App Server returns stored Task state immediately, then recovers the Native Session in the background without issuing `session/list`. When a matching cached Native Session exists, App Server compares its Agent-provided `updatedAt` with the Task's `localHistoryUpdatedAt`.

When the cached Native timestamp is present, comparable, and more than five seconds newer, App Server treats Chat as stale and calls `session/load` directly. The fixed tolerance absorbs normal delay between App Server persisting an Agent update and the Agent persisting its session timestamp. When Chat is not proven stale—including missing catalog data or missing, invalid, equal, older, or no-more-than-five-seconds-newer timestamps—App Server prefers `session/resume`, which restores the Native Session without replaying Chat. If the Agent does not advertise resume support, App Server falls back to `session/load`.

A successful resume applies any returned Configuration Options, attaches the Native Session update consumer, and leaves stored Chat unchanged. A load applies authoritative session catalogs, attaches the update consumer, and atomically replaces Chat with the replay.

When synchronization is required—either because Chat is stale or resume is unsupported—App Server publishes `historySync: syncing`, disables Send, and loads history in the background. Successful replay atomically replaces stored Chat with exactly the rendered `session/load` replay, sets `localHistoryUpdatedAt` to the load completion time, publishes a complete authoritative Task baseline, ends syncing, and enables Send. Failed replay keeps existing Chat, appends `History update failed` Live Activity, ends syncing, and enables Send.

Send and catalog refresh never check or initiate synchronization. A newer Native timestamp discovered while a Task stays open waits until that Task is opened again. Live updates for App Server-owned Native Sessions continue through their Native Session update consumers.

## Connection Authority

The resumable RPC session defined by [ADR 0026](0026-resumable-http-rpc-session.md) is connection authority. Frontend uses one logical `AppServerSession`; that module owns initialization, active scope replicas, cursor-gap recovery, subscription retry, and one request-readiness state across replaceable physical transports. Product consumers only map authoritative snapshots and events into render state. A completed empty poll and a retried upload are healthy transport behavior and do not invalidate Frontend state or show an App Server reconnect warning.

Temporary network failure is recovered inside the transport using sequence acknowledgement, replay, and duplicate suppression. Product mutations are never redispatched merely because an HTTP acknowledgement was lost. A lost session or changed App Server generation invalidates the replica and enters `resynchronizing`; non-idempotent requests with unknown outcomes are not replayed into the replacement process.

After transport-generation replacement, `AppServerSession` installs the replacement initialization result and exactly one authoritative baseline for every active scope behind a single recovery barrier. Product requests wait behind that barrier and the session becomes `ready` only after all current replicas are installed. A terminal replacement failure settles the barrier as `unavailable` rather than leaving requests pending. Late messages or baselines from an obsolete generation cannot clear recovery state.

A browser wake restarts the replayable receive poll, immediately probes product-client liveness, and invalidates active scope replicas for authoritative resubscription even when an idle Task has no incoming updates. Repeated wake signals coalesce behind one renewal. A wake that overlaps physical-generation recovery joins that recovery's initialization and scope-baseline barrier instead of starting a competing refresh cycle. Each scope gets at most five attempts with bounded exponential backoff; exhaustion stops request traffic and reports `unavailable`, while a later wake starts a fresh bounded renewal window. This read-only recovery never replays a product mutation and never clears a Frontend-owned Composer draft.

While the same Frontend process remains alive, local Composer state survives disconnect and resynchronization. The new baseline determines whether a New Task became visible with the accepted message or remains private and unsent. If product-client liveness expired, the former Prepared-Task lease is no longer trusted: Frontend forgets that lease, ignores its late scope baseline, and explicitly reacquires for the preserved Composer context after recovery becomes ready. App Server-provided attachment handles are invalidated because they belonged to the expired client session. Full Frontend reload never replays pending mutations; accepted work comes from App Server state, while unaccepted memory-only drafts may be lost as an explicit product trade-off.
