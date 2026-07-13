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

Opening an existing Task is the only automatic synchronization trigger. App Server reads the matching Native Session from the catalog cache without issuing `session/list` and compares its Agent-provided `updatedAt` with the Task's `localHistoryUpdatedAt`.

App Server calls `session/load` only when the cached Native timestamp is present, comparable, and more than five seconds newer. The fixed tolerance absorbs normal delay between App Server persisting an Agent update and the Agent persisting its session timestamp. Missing, invalid, equal, older, or no-more-than-five-seconds-newer timestamps require no synchronization.

When synchronization is required, App Server returns stored Chat with `historySync: syncing`, disables Send, and loads history in the background. Successful replay atomically replaces stored Chat with exactly the rendered `session/load` replay, sets `localHistoryUpdatedAt` to the load completion time, publishes a complete authoritative Task baseline, ends syncing, and enables Send. Failed replay keeps existing Chat, appends `History update failed` Live Activity, ends syncing, and enables Send.

Send and catalog refresh never check or initiate synchronization. A newer Native timestamp discovered while a Task stays open waits until that Task is opened again. Live updates for App Server-owned Native Sessions continue through their Native Session update consumers.

## Connection Authority

The event stream is connection authority. Stream failure immediately moves Frontend to `disconnected`, blocks Send, and leaves the Task, Chat replica, and Composer visible. A short presentation grace suppresses warnings for immediate recovery. A warning belongs to the failed connection generation and cannot reappear after a later navigation, unmount, reload, or successful generation.

While disconnected, Frontend retries only the read-only event stream with backoff. It does not call `state/subscribe` repeatedly, reload Task or Navigation snapshots, replay `task/send`, or start independent recovery loops for each scope.

When a replacement event stream connects, Frontend enters `resynchronizing` and assigns a new connection generation. It requests exactly one baseline for every still-active scope: normally Task Navigation and the open Task, plus each Tool detail that remains expanded. A baseline is accepted only while that generation remains connected. Failure during resynchronization invalidates incomplete and late results from that generation and returns Frontend to `disconnected`.

Frontend becomes `ready` and enables Send only after every active scope has installed its authoritative baseline under the still-connected generation. A successful request or late baseline from an obsolete generation cannot clear reconnect state.

While the same Frontend process remains alive, local Composer state survives disconnect and resynchronization. The new baseline determines whether a New Task became visible with the accepted message or remains private and unsent. Full Frontend reload never replays pending mutations; accepted work comes from App Server state, while unaccepted memory-only drafts may be lost as an explicit product trade-off.
