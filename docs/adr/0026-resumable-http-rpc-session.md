# Resumable HTTP RPC Session

Status: accepted

OpenAIDE presents one transport-independent peer API to Frontend and App Server code. Either peer may send a typed request or notification. A request handler's return value is its response; product code does not inspect envelopes or call a separate `respond()` operation. App events are notifications. Permission and Question decisions are ordinary typed client requests with atomic first-wins resolution in App Server.

The browser transport is a logical session over finite HTTP because supported IDE networks may reject WebSockets. A client opens a session, serializes sequenced JSON-RPC frames through `POST`, and receives the single ordered server-to-client stream through held `GET` polls. Upload responses are transport acknowledgements only and never carry JSON-RPC messages.

Each direction has an independent contiguous sequence. Retrying an upload repeats the identical frame; App Server acknowledges duplicates without dispatching them again. Poll acknowledgement is the highest fully applied server sequence, so an ambiguous poll can replay frames safely. Request IDs correlate RPC only and are not transport sequence numbers.

A session is bound to one authenticated client connection and one App Server generation. Temporary HTTP failures are hidden inside the adapter. An App Server generation change terminates the session: non-idempotent requests are not replayed into the new process, and Frontend obtains fresh authoritative baselines. Replay and queue storage must be bounded; falling outside retained history requires explicit resynchronization rather than silent loss.

Initialized App Shell clients collectively own physical App Server lifetime; no individual launcher or window owns the process generation. Authenticated transport activity, including reliable receive polls, renews product-client liveness. Missing activity first enters reconnect grace, and expiry of the last client after that grace lets the on-demand App Server shut down. Shell health probes remain separate from product-client liveness and do not keep an otherwise orphaned App Server alive.

Browser suspension can leave the held receive request unusable even while its reliable HTTP session remains valid. Browser wake, restored visibility, or network restoration aborts only that replayable receive request and immediately polls the same session from the highest fully applied server sequence. A client-side receive deadline slightly longer than the server hold deadline provides the same recovery when lifecycle signals are missed. Pending RPCs and events remain correlated because neither path replaces the logical session.

Suspension can also expire either the finite HTTP session or the initialized product client while the same App Server generation remains alive. After the replayable receive path exposes that expiry, the connection adapter replaces the HTTP session, verifies that the `serverId` is unchanged, repeats the stored `client/initialize`, and rebinds notification and inbound-request handlers. An RPC interrupted by HTTP `410 Gone` is not replayed because its dispatch outcome is ambiguous. A request rejected with protocol error `notInitialized` may be retried once after reinitialization because routing rejects it before product dispatch; this includes heartbeat-driven recovery before the user's next action.

The connection adapter publishes replacement initialization success or terminal failure to the logical `AppServerSession`. That session is the recovery boundary presented to Frontend: it invalidates every active scope replica, installs fresh baselines, and holds later product requests behind one barrier until the full active replica set is coherent. This rule is transport-independent and prevents each screen or subscription from inventing its own reconnect lifecycle.

The HTTP adapter is replaceable. A future WebSocket, IDE IPC, streaming-fetch, or WebTransport adapter must preserve the same peer and sequencing semantics, so product code is independent of physical transport.
