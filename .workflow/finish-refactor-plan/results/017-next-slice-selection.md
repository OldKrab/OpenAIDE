# P06 Next Slice Selection

Completed: 2026-06-26T20:37:47+03:00

## Selected Slice

Backend Task mutation commit seam and workflow split.

## Why This Slice

- It is the first narrow implementation slice inside "Backend Rust module split beyond
  the first skeleton"; doing a broad module split now would be mechanical churn.
- Storage concurrency is now protected, but Task workflows still repeat the shallow
  pattern of lock, mutate store, bump revision, write task, and notify from multiple
  call sites.
- The refactor plan already requires product-state writes to go through a commit seam
  that returns accepted mutation results and event or outbox facts before
  `state_sync.publish_committed`.
- `state_sync.publish_committed` exists, but Task workflow writes still use the legacy
  `RuntimeNotifier` path rather than an ordered committed-event interface.
- A deep mutation module gives callers one small interface for durable Task commits,
  revision assignment, message-history refresh, and publication facts.

## Non-Goals

- Do not redesign every Task workflow in one slice.
- Do not replace the protocol, storage file format, Agent runtime, or state-sync module.
- Do not add a durable outbox yet.
- Do not create abstract ports for storage or notifier unless there are real adapters.

## Next

Proceed to `P02-api-grill-next-slice`: grill and record the Task mutation commit
interface before implementing any workflow rewrites.
