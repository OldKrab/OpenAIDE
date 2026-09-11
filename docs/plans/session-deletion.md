# Session deletion

Status: agreed contract implemented; final repository checks and review in progress.

## Agreed scope

- Delete removes a Task or discovered Native Session from OpenAIDE and asks its Agent to remove the Native Session from Agent history. Agent-retained data may remain; Codex implements this through thread archiving. The confirmation must explain that retention boundary.
- OpenAIDE Archive remains a separate, local, reversible action.
- Delete covers both adopted Tasks and unadopted Native Sessions in the normal list and Archive, and is offered only for Agents advertising session deletion support.
- Allowing Delete in Archive expands the accepted Restore-only rule.
- Agent rejection or an unknown transport outcome keeps the entry visible with an explicit failure or unknown-outcome message and an explicit retry action. ADE must not report successful deletion or remove local history before Agent success is confirmed. This replaces ADR 0015's local-first failure policy for explicit user deletion.
- Active work does not require a separate Stop or resolution of pending permission requests before Delete. After the normal deletion confirmation, active work requires a second confirmation. ADE then asks the Agent to delete the session; the Agent owns whether and how to terminate its work. If the Agent rejects active-session deletion, retain the entry and expose the failure; revisit this UX based on observed failures.
- The deletion confirmation names the queued-message count when queued work will be discarded.
- Successful deletion removes local Task metadata, Chat, Tool artifacts, queued messages, and that Task's Composer History entries. Project files and worktrees are preserved. Purging Composer History explicitly changes the current lifecycle specification's retention rule.
- Offer single-entry Delete in Task/session menus across app shells, including Archive, using destructive red styling. Both Archive and Delete show explanatory confirmation popups that distinguish local reversible Archive from removal from ADE and Agent history. No bulk deletion initially.
- After successful deletion, remove the entry from all clients and move any client viewing the deleted entry to New Task.
- App Server rechecks activity before dispatch and requires the extra confirmation if work started after the initial confirmation. Once deletion begins, block new work for that session while the operation is unresolved.
- Do not introduce a special durable remote-deletion recovery mechanism. Improve general missing-session behavior so the ordinary lifecycle handles stale local entries, including after a crash between Agent success and local cleanup.
- Purge Task-owned history, inline images, and Tool artifacts; release ephemeral attachment handles. Preserve referenced filesystem files and the existing temporary-upload lifetime. Do not add shared-file reference tracking for Delete.
- Reuse the Native Session Catalog's existing Agent listing observations to reconcile stale entries, including Archived Tasks; do not add separate per-Task validation requests just for browsing Archive.
- Automatically remove a missing Native Session's local Task and saved history, including Archived Tasks, when authoritative evidence establishes removal from Agent history. A definitive missing response on an ordinary session open qualifies. Listing absence qualifies only after an existing refresh successfully traverses from the first page to the Agent's actual terminal cursor in the same Agent/workspace/filter scope.
- Record genuine listing completion separately from early termination. Bounded, failed, cyclic, and no-progress scans cannot establish absence. Keep existing bounded refresh demand; do not fetch extra pages for deletion reconciliation. Cleanup can therefore wait until a complete scan occurs.

## Pre-implementation findings

- The pinned `@openaide/codex-acp` 1.2.2 package advertises and implements session deletion through Codex thread archiving.
- OpenAIDE has no exposed deletion protocol method or navigation action. It already projects the Agent deletion capability.
- The internal Task deletion helper commits a local tombstone before attempting Agent deletion and discards the Agent result. Repeating the operation does not retry Agent deletion.
- The current ACP deletion path requires an attached session. Its five-second result timeout can leave the remote outcome unknown, and failure still discards the attachment.
- Existing Task tombstone/purge maintenance recovers interrupted local physical cleanup once a tombstone is durable. Task storage owns Chat, inline images, Tool journals, and per-Task Composer History; Project Composer History aggregates contributions from retained Tasks.
- Referenced files are not Task-owned bytes. Web uploads live under OS temp outside Task storage, and forked Agent histories may retain references to them; an originating Task directory does not establish exclusive ownership.
- Existing missing-session handling does not remove an adopted Task: open failures retain it, and opening an Archived Task does not contact the Agent. Only failed adoption of an unadopted Native Session removes its catalog row on definitive not-found (`tasks/product_api/open.rs`, `adopt_native_session.rs`). Thus ordinary next-open handling does not guarantee cleanup after remote deletion succeeds and ADE crashes before local removal. ACP also leaves loading a deleted session implementation-defined.
- Listing refresh is bounded and merges successful pages into durable cache. Its current stop outcomes conflate provider exhaustion with repeated cursors or pages containing no new identities; `has_more=false` is not proof of a complete listing. Absence is usable only after a successfully completed scan of the same Agent/workspace/filter scope, and establishes absence from that listing rather than permanent erasure. Existing observations can support this without extra requests when a scan naturally completes, but their completeness must be represented accurately.

## Implementation outline

- App Server owns capability checks, target resolution, confirmation/activity checks, operation ordering, Agent deletion, cleanup, and publication. Use the same Task-or-Native-Session target vocabulary across app shells.
- Support both attached and detached Native Sessions without adopting an unowned session merely to delete it. Agent deletion success precedes the existing durable local tombstone/purge path. Expose rejection and unknown outcomes without silently losing the entry or retrying the Agent mutation.
- Extend general reconciliation with accurately classified listing completion and ordinary definitive missing responses. A Codex archived-session load error is not automatically not-found; completed normal-history enumeration can establish its absence without relying on provider error wording.
- Reuse existing local physical-cleanup recovery and authoritative removal publication. Preserve newer session observations and restrict absence reconciliation to identities covered by the completed scan's scope.
- Add shared navigation actions and explanatory confirmation popups, including the second confirmation for active work and the queued-message count.

## Verification targets

- Capability gating; normal and archived Tasks; unadopted sessions; attached and detached deletion.
- Active-work double confirmation and work starting concurrently; queue handling; multi-client removal/navigation.
- Agent success, rejection, and unknown outcome; explicit retry; local cleanup recovery after a durable tombstone.
- Genuine complete-scan absence versus bounded, failed, repeated-cursor, no-progress, and changed-scope scans; preservation of newer observations.
- Definitively missing Task open; purging Task-owned history and Composer History while retaining referenced files and independent contributions from other Tasks.

The accepted contract is documented in ADR 0059 and the lifecycle specification.

## Implementation and verification

- Added the shared `nativeSession/delete` intent, capability-gated navigation actions, explanatory Archive/Delete dialogs, active-work confirmation, and authoritative removal navigation.
- Agent success precedes local tombstone/purge. Rejection retains usable history; unknown outcomes require explicit retry and block further session work. Attached deletion continues consuming ordered ACP responses while waiting for the Delete reply.
- Existing complete history scans and definitive ordinary-open missing responses reconcile saved history. Observation generations and Task operation/revision guards preserve newer evidence. Listing absence waits until live turns settle because new Agent sessions can precede their durable history entries.
- App Server/ACP regression tests cover deletion success, rejection, uncertainty, admission/queue races, missing-session classification, scan completeness, newer observations, Composer History ownership, and referenced-file retention.
- Shared frontend tests cover confirmations, retry, menus, and shell composition. Isolated browser checks passed for wide archived deletion with two viewing clients and narrow active deletion.
- Browser failures now attach App Server diagnostics before the isolated harness deletes its state, preserving the signal needed to diagnose protocol stalls.
