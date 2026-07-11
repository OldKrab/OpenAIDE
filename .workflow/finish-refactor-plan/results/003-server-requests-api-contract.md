# P02 Server Requests API Contract

Completed: 2026-06-26T19:04:15+03:00

## Accepted

- `server_requests` is the single App Server module for live Backend-initiated
  Frontend/App Shell requests: `permission/*`, `secret/*`, and `shell/*`.
- Main external module is `server_request_broker`.
- The broker owns pending request records, responder eligibility, request ids, delivery
  attempts, first-valid-response-wins, late/stale response errors, interruption state,
  and safe pending request snapshot rows.
- Callers open requests and handle closed broker outcomes; they do not create their own
  waiters or pending maps.
- Opening a request is non-blocking and returns delivery instructions or a structured
  unavailable outcome.
- Delivery is separate from opening and is performed by the protocol edge through current
  client delivery ports.
- Task-scoped requests survive individual client disconnects and temporary lack of Task
  subscribers while the App Server remains alive.
- Client-scoped requests fail or interrupt when their target client disconnects before
  answering.
- Pending requests are live memory state only; restart recovery belongs to Task recovery
  workflows, not the broker.

## Rejected

- Did not let Task workflows, Agent runtime, or attachment runtime own independent
  request waiters.
- Did not let the broker execute shell capabilities, persist recovery state, perform ACP
  I/O, own subscriptions, own client identity, or publish durable product events.
- Did not implement code in this packet.

## Review

Local review checked the contract against `AGENTS.md`, `CONTEXT.md`,
`docs/refactor-plan.md`, and ADR 0022. One correction was made from ADR 0022: Task-scoped
requests remain pending even when no clients are currently subscribed to the Task, as long
as the App Server process remains alive and later clients can resubscribe.

## Test Obligations For P03

- Opening is non-blocking.
- Client-scoped request fails on target client disconnect.
- Task-scoped request survives one client disconnect.
- Task-scoped request remains pending without current subscribers.
- First valid Task-scoped response wins.
- Late response returns a stable stale/resolved error.
- Unauthorized client cannot answer.
- Interruption prevents later mutation.
- Snapshot projection includes only safe rows.
- Lifecycle redelivery happens when an eligible client appears.

## Next

Proceed to `P03-implementation-slice`: implement only the accepted `server_requests`
module slice and focused tests.
