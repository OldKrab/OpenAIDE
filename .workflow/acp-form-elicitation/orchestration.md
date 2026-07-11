# Orchestration

1. Confirm live RFD and pinned SDK types; record any differences.
2. Implement backend/protocol and frontend tracks in disjoint ownership.
3. Integrate generated bindings and resolve interface mismatches at the typed App Server Protocol seam.
4. Run focused tests, then broader checks; fix failures without reverting unrelated worktree changes.
5. Redeploy only the target instance and exercise all user-visible states in a browser.

Integration policy:

- App Server Protocol carries normalized product form types, never raw JSON Schema.
- App Server validates request budgets before opening a pending request and validates accepted content before resolving it.
- A successful client response remains visually pending/responding until the durable terminal snapshot arrives.
- Concurrent requests are keyed and awaited independently; no lock is held while waiting for user input.
