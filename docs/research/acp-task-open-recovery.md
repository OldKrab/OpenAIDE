# ACP task-open recovery: fast controls without stale replay rollback

Research date: 2026-08-11

## Question

How should OpenAIDE reopen an existing Task quickly when ACP history may have changed outside OpenAIDE, without allowing a later replay to revert Configuration Options or other state accepted after reconnection?

This note combines the captured `task_33f927d1-47cf-44fa-97ca-6676fe001773` incident, the current OpenAIDE implementation and accepted Task Chat specification, and the official ACP v1 and v2 contracts.

## Conclusion

The best ACP v1 design is **resume-first recovery with one restore per attachment and deferred replay**:

1. If no durable replay requirement is already known, reconnect a known Task binding immediately with `session/resume`; do not make editable controls wait for `session/list`.
2. Establish an attachment epoch and replay fence before publishing controls as ready. Invalidate them before every user/session operation and on every live Agent update that advances the Task's session projection.
3. Observe the catalog in the background and treat `updatedAt` only as a possible-staleness hint. A newer hint marks replay as deferred; it does not automatically call `session/load` on the resumed attachment.
4. Perform `session/load` only as a new, explicit recovery attempt: when the user chooses **Reload session**, or on the next Task open when a durable deferred-replay flag is present. Close the old attachment, load once, and keep controls locked until that load becomes authoritative.
5. When already-cached evidence says replay is required before attachment, choose one direct `session/load` instead of resume followed by load. When resume is unsupported, load before enabling controls.

This reduces the captured controls recovery path from the approximately 20-second pre-resume gap to the actual resume time, 568 ms in this incident. It also closes a gap in the current rule: today's passive token is created when a later observation arrives, so it does not remember interactions or Agent updates that happened earlier in the attachment.

An automatic `session/load` after a successful `session/resume` is not protocol-safe, even when a local fence is still pristine. The fence can prove that OpenAIDE has not accepted a newer local operation; it cannot prove that two Agent restore snapshots have identical Configuration Options or other state. ACP has no history revision, compare-and-swap field, causal token, or replay-only operation with which to order them. The unavoidable ACP v1 tradeoff is explicit: OpenAIDE can restore controls immediately or guarantee replay before interaction, but cannot silently promise both.

## What happened in the captured incident

The driver diagnostics show this sequence:

| Event | UTC time | Result |
| --- | --- | --- |
| `task/open` started | 09:37:55.568 | Stored Task returned asynchronously |
| `task/open` completed | 09:37:55.704 | 135 ms |
| `native_session_open_resume_started` | 09:38:15.114 | Recovery had spent 19.410 s before resume began |
| `native_session_open_resume_timing` | 09:38:15.683 | Resume 559 ms, 568 ms total; config and command catalogs returned |
| Global project-tree catalog refresh completed | 09:38:30.277 | 814 sessions discovered |

Transport health polls continued during the gap, and the Task storage and Chat artifacts had no structural or RPC failure. On a warm browser reload, the same Task recovered controls in about 1.8 seconds; the backend resume itself took 17 ms after a 139 ms `task/open`.

The gap is explained by current ordering:

- `TaskProductApi::open_task` returns cached state and starts `spawn_adopted_task_refresh` (`openaide-rs/app-server/src/tasks/product_api/open.rs`).
- That worker calls `refresh_bound_session_for_open` before choosing resume or load. The function walks opaque ACP pages until it finds the known session id or exhausts the result (`openaide-rs/app-server/src/tasks/product_api/list_sessions.rs`).
- ACP's standard list request has only optional `cwd` and `cursor`; it has no direct session-id filter, page size, or guaranteed ordering. A correct targeted lookup may therefore traverse every page. [ACP v1 Session List](https://agentclientprotocol.com/protocol/v1/session-list#listing-sessions)
- Global navigation discovery also performs paginated listing. Global and task-targeted demand do not share one coalesced lookup, while `AcpRuntimeKernel::list_sessions` holds the process-operation mutex for each request (`openaide-rs/app-server/src/agent/acp_runtime_kernel.rs`). A targeted lookup can therefore wait behind global work and then perform its own page walk.

The diagnostics currently do not record lookup start, mutex wait/acquisition, page completion, or match/not-found. The logs prove that the 19.410-second gap occurred before resume and coincided with catalog work, but cannot divide it exactly between mutex wait and ACP page time.

## What ACP guarantees

### Resume does not require list

For a known session id, ACP v1 `session/resume` directly accepts `sessionId`, `cwd`, and MCP servers. It restores context and must not replay prior conversation history before responding. `session/list` is explicitly a discovery operation that does not restore or modify a session. [ACP v1 Session Setup](https://agentclientprotocol.com/protocol/v1/session-setup#resuming-sessions), [ACP v1 Session List](https://agentclientprotocol.com/protocol/v1/session-list#interaction-with-other-session-methods)

Therefore the pre-resume scan is OpenAIDE consistency policy, not an ACP prerequisite.

### Load is another restore, not a read-only replay

ACP v1 `session/load` restores an existing session and must replay its entire conversation through `session/update` before returning. `session/resume` restores without that replay. ACP does not specify the causal semantics of resume followed by load, or of a load racing with configuration mutation. [ACP v1 Session Setup](https://agentclientprotocol.com/protocol/v1/session-setup), [Session Resume RFD](https://agentclientprotocol.com/rfds/session-resume)

Both load and resume responses may include Configuration Options with `currentValue`. A `session/set_config_option` response and an Agent `config_option_update` notification each carry a complete current catalog, but no response includes a revision that orders it relative to recovery. [ACP v1 Session Config Options](https://agentclientprotocol.com/protocol/v1/session-config-options), [ACP v1 Schema](https://agentclientprotocol.com/protocol/v1/schema#session%2Fload)

ACP requires conversation replay during load, but does not explicitly guarantee replay of the historical sequence of configuration notifications. A client should not assume that history replay will reconstruct or preserve a newer option change.

### `updatedAt` cannot prove history freshness

ACP defines `SessionInfo.updatedAt` as an optional time of last activity. It does not define which activity changes it, monotonicity, precision, or a relation to message replay. `SessionInfo` has no history revision, last-message id, message count, content hash, or standardized replay cursor. [ACP v1 Session List](https://agentclientprotocol.com/protocol/v1/session-list#listing-sessions), [ACP v1 Schema](https://agentclientprotocol.com/protocol/v1/schema#sessioninfo)

OpenAIDE already acknowledges this limitation in `docs/task-chat-flow.md`: a newer catalog timestamp can represent history or another external session change. Comparing it with the local Chat clock is a useful hint, not a protocol-safe proof.

### ACP v2 does not yet provide incremental catch-up

ACP v2 unifies load and resume: `session/resume` without `replayFrom` reconnects without replay, while `{ "type": "start" }` requests full replay before the response. The only standardized replay cursor is currently `start`; message, checkpoint, and server cursors remain future possibilities. [ACP v2 Session Setup](https://agentclientprotocol.com/protocol/v2/session-setup#resuming-sessions), [ACP v2 Session Resume Replay RFD](https://agentclientprotocol.com/rfds/v2/session-resume-replay)

That removes the two-method API shape but does not yet provide “attach now, catch up from revision X later.” OpenAIDE currently exports ACP v1 schema types (`openaide-rs/app-server/src/agent/acp_schema.rs`), so v2 is a future direction rather than a present fix.

## The race in the current OpenAIDE design

OpenAIDE has the right basic primitive but captures it too late for later observations:

- `TaskOperationCoordinator::serialize` increments an interaction generation before a serialized Task operation.
- `HistorySyncCoordinator::begin_passive` captures that generation.
- `try_serialize_passive` abandons replacement if a user operation has changed the generation or currently owns the gate.
- Send/turn acceptance, Configuration Option mutation, cancellation, Archive, and related session operations share this coordinator.

This protects a passive replacement only against operations occurring **after that passive observation begins**. It does not establish a baseline when the attachment is resumed. A user can select an option after resume, then a later catalog observation can create a fresh passive token and load the session because the token considers that already-completed option change part of its baseline.

Live Agent updates are a second gap. `TaskSessionEventSink` writes configuration, command, metadata, and message updates directly through Task mutations; it has no reference to the operation generation. A later passive token therefore cannot tell that the current attachment advanced through an Agent notification before catalog observation.

The accepted specification says that a user operation wins over passive replacement, but this late-token behavior does not fully implement the intended attachment-wide meaning of “wins.”

## Recommended state machine

Keep the App Server as the sole owner. Frontend should render authoritative state and never implement a timeout or recovery race locally.

### Open

1. Return stored Chat immediately.
2. Report controls as `recovering` and history verification as `checking`.
3. If a durable deferred-replay flag or cached catalog evidence already requires authoritative replay, call `session/load` once under the Task gate.
4. Otherwise call `session/resume` directly using the bound id.
5. Apply the response, attach the update consumer, create a new attachment epoch with a pristine replay fence, and publish controls as ready.
6. Let the sole Native Session Catalog owner satisfy one coalesced background observation. Task open registers interest in a known id; it does not start a second independent page walk.

### Fence invalidation

Invalidate the attachment fence before Agent I/O for any operation that can advance or depend on session state:

- Configuration Option changes;
- Send, steer, commands, cancellation, and permission/question continuations where relevant;
- Archive, close, replacement, or any attachment change;
- every accepted live Agent event that changes Chat, Configuration Options, commands, title, capabilities, plan, or other replayable session projection.

The invalidation and operation must share the per-Task gate. The fence rejects delayed work from an older attachment, but it is not permission to perform a second automatic restore. A successful explicit or open-time load creates a new attachment epoch and a new pristine fence.

### Background observation

When catalog activity appears newer than the local projection:

- **Any successfully resumed attachment:** do not auto-load. Persist a deferred-replay requirement and offer an explicit **Reload session** action. If the user takes it, publish `reloading`, lock controls and Send, close the attachment, load once, and atomically replace Chat and catalogs as a new attachment.
- **Invalid or superseded fence:** discard delayed recovery work in addition to marking replay deferred; never apply its result to the newer attachment.
- **Missing or incomparable timestamp:** do not perform destructive replacement.

Because `updatedAt` is generic, UI copy should say **Session changes may be available** or **History verification deferred**, not claim that new messages exist. On the next ordinary Task open, the durable deferred-replay requirement chooses load before controls become editable, so ignoring the action does not lose the need for reconciliation.

### Separate visible state axes

“Refreshing options” currently conflates connection recovery with history reconciliation. Model them separately:

| Axis | Suggested states |
| --- | --- |
| Live session controls | `recovering`, `ready`, `stale` |
| History reconciliation | `unchecked`, `checking`, `current`, `reloading`, `deferred`, `failed` |

This allows controls to become editable as soon as resume succeeds while background discovery remains slow or unavailable.

## Alternatives considered

| Design | Latency | Consistency and safety | Verdict |
| --- | --- | --- | --- |
| Current list-before-exactly-one-restore | Can block controls on every catalog page; 19.410 s before resume here | Avoids two restores during open, but later passive tokens do not cover the whole attachment and `updatedAt` is ambiguous | Replace |
| Always load on Task open | Avoids list; cost grows with replay and projection commit | One authoritative operation and no post-resume rollback, but unnecessarily replays every history and excludes resume-only Agents | Useful fallback, not default |
| Resume then unconditional load | Fast initial controls followed by another restore | Can overwrite newer options/state; ACP supplies no causal ordering | Reject |
| Resume then auto-load only while a local fence is pristine | Resume latency for controls; discovery no longer blocks | Protects post-resume local actions, but ACP still cannot prove that the two restore snapshots agree | Reject as a protocol guarantee |
| Resume plus deferred explicit/next-open replay | Resume latency for controls; discovery no longer blocks | One restore per attachment; newer activity becomes a visible durable recovery requirement | Recommended for ACP v1 |
| Agent-specific direct history revision/cursor | Potentially fast and precise | Not portable ACP v1; requires an extension | Long-term optimization |
| ACP v2 replay cursor | Unified API | Only full replay from start is standardized today | Track, not a current solution |

Historical OpenAIDE diagnostics also show why “always load” is not a universal latency fix. Observed full loads ranged from roughly 252 ms for 16 messages to 7.172 s for about 1,294 messages, with projection commits sometimes adding several seconds. These are incident observations rather than controlled benchmarks, but they demonstrate history-size-dependent work that resume avoids.

## Module boundaries

The clean ownership split is:

- **Native Session Catalog:** the only `session/list` caller; coalesces global and known-id observation demand, owns pagination and cached observations, and publishes metadata-only progress.
- **Native Session Recovery:** owns one restore per attachment, resume/load/close lifecycle, attachment epoch, replay fence, durable deferred-replay requirement, and authoritative replacement.
- **Task operation coordinator:** serializes session-affecting operations and validates attachment fences under the same gate.
- **Task session update consumer:** projects live Agent updates and invalidates the current attachment fence before publishing them.
- **Frontend:** renders the two state axes and explicit recovery action; it does not infer freshness.

This retains one owner for each concern and avoids teaching Task open, navigation, and Frontend separate pagination or recovery rules.

## Diagnostics required before rollout

Add structured lifecycle events around the currently invisible boundary:

- `bound_session_observation_started` and terminal outcome;
- process-operation mutex wait and acquired durations;
- each list page completion with page index, safe count, next-page boolean, and duration;
- bound id found, exhausted, failed, or coalesced;
- recovery decision: cached/deferred direct load, resume, resume fallback load, or deferred replay;
- attachment epoch creation and replay-fence invalidation with a classified reason, never values or content;
- passive replacement won, skipped, or deferred;
- resume/load/commit/attachment terminal timing.

These events would make the first delayed boundary directly observable instead of inferring it from the gap between `task/open` and resume.

## Implementation implications

This recommendation changes the accepted ordering in `docs/task-chat-flow.md`: Task open would no longer require a targeted fresh observation before choosing resume or load, and later reconciliation would stop automatically replacing a successfully resumed attachment. It would instead gain an attachment epoch plus a durable deferred state. It therefore needs explicit product agreement before production code changes.

The closest regression tests should prove real boundaries:

1. a slow multi-page catalog cannot delay a supported direct resume or controls readiness;
2. a newer background observation after resume persists deferred replay and never calls load;
3. an option selected after resume remains final while replay is deferred;
4. a live Agent configuration or message update remains final while replay is deferred;
5. an explicit reload closes the old epoch, loads exactly once, and keeps controls locked until completion;
6. the next open with deferred replay loads directly without first resuming;
7. a stale recovery result from an old attachment epoch is discarded;
8. unsupported resume loads before controls become editable;
9. global and targeted catalog demand share one page stream rather than duplicate it.
