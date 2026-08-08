# Task State Publication and Replica Recovery

Status: accepted

This ADR defines the ordered publication, history-synchronization, and connection-recovery architecture that supports the accepted [Task Lifecycle and Chat Specification](../task-chat-flow.md).

## Scope-Local Publication

- `state/subscribe` returns one complete authoritative baseline and its scope-local revision. Baselines are used for initial subscription, stream replacement, revision-gap recovery, and explicit complete history replacement.
- Each subscribed scope has an independent ordered stream. A Task scope does not advance through unrelated Project, Agent, Navigation, or other Task events.
- Each durable transaction that changes `TaskSnapshot` increments one Task revision and publishes exactly one `taskChanged` event. Tool-detail-only terminal appends advance only their independently ordered Tool-detail scope, as clarified by ADR-0028.
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

`NativeSessionCatalog` is the only owner of ACP `session/list`. It durably stores successful observations by Agent identity and Native Session identity, with Project and canonical Task Workspace context. The Task Navigation snapshot merges those observations with durable Tasks into one activity-sorted list and excludes sessions already bound to a Task.

The catalog also accepts normalized `session_info_update` metadata from live Task runtimes. Activity advances monotonically, and a later listing page may replace cached title or activity only when it carries newer activity evidence; a lagging `session/list` response cannot roll a live observation backward. Task title provenance remains independently authoritative for the owned Task.

Discovery covers every enabled Agent across each visible Project root and available worktree. Independent Agent/workspace requests run in parallel with a global bound of 20. Successful pages persist and publish independently; omitted rows and failed requests never delete cached observations. A page containing no new identity in its live cursor generation stops that generation. App Server owns all cursors and process-local Project depth high-water marks.

Bounded discovery validates each raw Agent page before normalization. Descending, timestamped pages remain trusted and may stop when their oldest activity cannot beat the requested Project cutoff. Missing or invalid activity, ascending rows, or a later page crossing the prior page frontier makes that listing generation untrusted; equal timestamps and duplicate identities do not. Every generation still stops at its requested depth, cursor exhaustion, or a page with no new identity.

It refreshes active catalogs:

- once when Task Navigation gains a subscriber and once every five minutes while subscribers remain;
- on explicit user Refresh.

`taskNavigation/loadMore` raises a Project's process-local depth target without exposing Agent cursors to Frontend. Concurrent requests coalesce with one trailing generation. Disabling or removing an Agent hides both its durable Tasks and unadopted sessions without deleting retained data.

Each result reconciles Agent title metadata and advances a bound Task's activity only when the Agent timestamp is newer. After committing the complete listing observation, App Server checks subscribed, idle bound Tasks for stale Chat and may initiate Task history synchronization.

## Native Session Synchronization

Opening an existing Task returns stored state immediately and asks the Native Session Catalog module for one targeted fresh observation of the bound identity. Native Session recovery waits behind that background observation, not the Task response. When listing fails or succeeds without that identity, recovery falls back to `session/resume`. A later complete catalog observation also checks an idle subscribed Task. When a matching Native Session exists, App Server compares its Agent-provided `updatedAt` with the Task's `localHistoryUpdatedAt`.

When the fresh Native timestamp is present, comparable, and more than five seconds newer, App Server treats the complete local Native Session projection as stale. The fixed tolerance absorbs normal delay between App Server persisting an Agent update and the Agent persisting its session timestamp. When staleness is not proven—including missing, invalid, equal, older, or no-more-than-five-seconds-newer timestamps—App Server prefers `session/resume`. If the Agent does not advertise resume support, App Server falls back to `session/load`.

A successful resume applies any returned Configuration Options, attaches the Native Session update consumer, and leaves stored Chat unchanged. A load is never issued through an already-live attachment. App Server first detaches that attachment, sends `session/close` when the Agent supports it, and creates a new attachment through `session/load`. The complete loaded session state is authoritative; Chat replay and all session metadata publish atomically.

One per-Task lifecycle gate orders passive replacement against every user session operation. A user operation that acquires the gate first invalidates scheduled passive work; synchronization waits until the next Task open. Replacement that acquires it first marks catalogs recovering and holds later operations until the new attachment commits. Frontend keeps last-known controls visible but locked and labels the operation **Reloading session**.

When synchronization is required—either because the Native projection is stale or resume is unsupported—App Server publishes `historySync: syncing`, disables Send, and loads in the background. Successful replay replaces stored Chat, catalogs, and session metadata in one authoritative Task commit, sets `localHistoryUpdatedAt`, ends syncing, and enables Send. Failed replay keeps existing Chat, appends `History update failed` Live Activity, marks cached controls stale, ends syncing, and enables explicit recovery.

Send never checks or initiates synchronization. After a complete listing observation updates the catalog cache, a newer Native timestamp initiates synchronization for a subscribed, idle Task. Unsubscribed, Archived, Prepared, Starting, Active, or active-turn Tasks are not refreshed by catalog discovery. Live updates for App Server-owned Native Sessions continue through their Native Session update consumers.

## Connection Authority

The resumable RPC session defined by [ADR 0026](0026-resumable-http-rpc-session.md) is connection authority. Frontend uses one logical `AppServerSession`; that module owns initialization, active scope replicas, cursor-gap recovery, subscription retry, and one request-readiness state across replaceable physical transports. Product consumers only map authoritative snapshots and events into render state. A completed empty poll and a retried upload are healthy transport behavior and do not invalidate Frontend state or show an App Server reconnect warning.

Temporary network failure is recovered inside the transport using sequence acknowledgement, replay, and duplicate suppression. Product mutations are never redispatched merely because an HTTP acknowledgement was lost. A lost session or changed App Server generation invalidates the replica and enters `resynchronizing`; non-idempotent requests with unknown outcomes are not replayed into the replacement process.

Product-client expiry cleans up client-scoped product state and, after reconnect grace, last-client expiry lets the on-demand App Server shut down. Authenticated transport activity such as reliable receive polls renews liveness, so UI inactivity without transport loss is not an expiry signal. App Shells supervise endpoint health independently and supply a replacement endpoint to the existing logical `AppServerSession` after bounded probe failure.

After transport-generation replacement, `AppServerSession` installs the replacement initialization result and exactly one authoritative baseline for every active scope behind a single recovery barrier. Product requests wait behind that barrier and the session becomes `ready` only after all current replicas are installed. A terminal replacement failure settles the barrier as `unavailable` rather than leaving requests pending. Late messages or baselines from an obsolete generation cannot clear recovery state.

While the same Frontend process remains alive, local Composer state survives disconnect and resynchronization. The new baseline determines whether a New Task became visible with the accepted message or remains private and unsent. If product-client liveness expired, the former Prepared-Task lease is no longer trusted: Frontend forgets that lease, ignores its late scope baseline, and explicitly reacquires for the preserved Composer context after recovery becomes ready. App Server-provided attachment handles are invalidated because they belonged to the expired client session. Full Frontend reload never replays pending mutations; accepted work comes from App Server state, while unaccepted memory-only drafts may be lost as an explicit product trade-off.
