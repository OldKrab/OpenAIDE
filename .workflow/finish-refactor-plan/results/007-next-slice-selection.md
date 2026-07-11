# P06 Next Slice Selection

Completed: 2026-06-26T19:31:00+03:00

## Selected Slice

Backend process lifecycle, shared-instance discovery, and state roots.

## Why This Slice

- It is next in the module grill queue after `server_requests`.
- Current `app_lifecycle` only covers minimal running/draining/stopping admission.
- `client_lifecycle` already owns initialized client facts and reconnect grace, but the
  API contract for using those facts to drive shared App Server lifetime is not fully
  recorded.
- Shared attach-or-launch, state-root identity, runtime endpoint records, and storage
  concurrency protection must be designed before Web/Desktop/VS Code shells can safely
  reuse one local App Server and shared storage.

## Next

Proceed to `P02-api-grill-next-slice`: grill only the important API boundary for
process lifecycle, shared-instance discovery, and state roots. Do not implement
discovery, locking, endpoint records, or storage changes until the contract is accepted.
