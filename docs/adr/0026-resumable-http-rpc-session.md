# Resumable HTTP RPC Session

Status: accepted

OpenAIDE presents one transport-independent peer API to Frontend and App Server code. Either peer may send a typed request or notification. A request handler's return value is its response; product code does not inspect envelopes or call a separate `respond()` operation. App events are notifications. Permission and Question decisions are ordinary typed client requests with atomic first-wins resolution in App Server.

The browser transport is a logical session over finite HTTP because supported IDE networks may reject WebSockets. A client opens a session, serializes sequenced JSON-RPC frames through `POST`, and receives the single ordered server-to-client stream through held `GET` polls. Upload responses are transport acknowledgements only and never carry JSON-RPC messages.

Each direction has an independent contiguous sequence. Retrying an upload repeats the identical frame; App Server acknowledges duplicates without dispatching them again. Poll acknowledgement is the highest fully applied server sequence, so an ambiguous poll can replay frames safely. Request IDs correlate RPC only and are not transport sequence numbers.

A session is bound to one authenticated client connection and one App Server generation. Temporary HTTP failures are hidden inside the adapter. An App Server generation change terminates the session: non-idempotent requests are not replayed into the new process, and Frontend obtains fresh authoritative baselines. Replay and queue storage must be bounded; falling outside retained history requires explicit resynchronization rather than silent loss.

Browser suspension can expire either the finite HTTP session or the initialized product client while the same App Server generation remains alive. The connection adapter replaces the HTTP session, verifies that the `serverId` is unchanged, repeats the stored `client/initialize`, and rebinds notification and inbound-request handlers. An RPC interrupted by HTTP `410 Gone` is not replayed because its dispatch outcome is ambiguous. A request rejected with protocol error `notInitialized` may be retried once after reinitialization because routing rejects it before product dispatch; this includes heartbeat-driven recovery before the user's next action.

The HTTP adapter is replaceable. A future WebSocket, IDE IPC, streaming-fetch, or WebTransport adapter must preserve the same peer and sequencing semantics, so product code is independent of physical transport.
