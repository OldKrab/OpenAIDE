# Browser–App Server messaging without WebSockets

Research date: 2026-07-14

## Conclusion

A browser can have a stable **logical** bidirectional session with the App Server without WebSockets. It cannot have a server-initiated HTTP response without first making a request, but that does not prevent full peer semantics. BOSH explicitly standardizes emulation of a long-lived bidirectional TCP stream over synchronous HTTP request/response pairs, including request sequencing, acknowledgements, retries, and cached responses after broken connections ([XEP-0124](https://xmpp.org/extensions/xep-0124.html)). Engine.IO independently uses the same practical shape: long-running `GET` requests receive server packets, short `POST` requests send client packets, and one active request in each direction preserves ordering ([Engine.IO protocol](https://github.com/socketio/engine.io-protocol)).

For OpenAIDE's restrictive-network case, the recommended baseline is therefore:

> A resumable, BOSH-inspired HTTP long-poll session carrying JSON-RPC: one held receive request, one serialized upload request, one sequence space per direction, bounded replay, duplicate suppression, and explicit resynchronization.

Use complete long-poll responses by default. Make a fetch/SSE response stream an optional lower-latency adapter after an end-to-end capability test. RFC 6202 reports that most proxies handle long polling well because each delivery is a complete HTTP response, while an intermediary is allowed to buffer an entire partial streaming response rather than forward chunks promptly ([RFC 6202, proxy and streaming analysis](https://www.rfc-editor.org/rfc/rfc6202.html#section-5.3)). Thus SSE alone is the weaker baseline specifically for a network already known to restrict upgraded or long-lived communication.

This is not less correct than a WebSocket session. Either physical transport can disconnect. Correctness comes from explicit session identity, ordering, acknowledgement, replay, deduplication, and authoritative state recovery.

## Recommended protocol shape

Keep JSON-RPC transport-independent. The JSON-RPC specification itself is transport agnostic and permits one implementation to act as both client and server; a method message with `id` is a request, a method message without `id` is a notification, and a response repeats the request ID ([JSON-RPC 2.0](https://www.jsonrpc.org/specification)). Events are ordinary JSON-RPC notifications.

Use three HTTP operations:

```text
POST /rpc/sessions
  Establish or replace a logical session.

GET /rpc/sessions/{sessionId}/messages?after={serverSeq}&wait=25
  Hold until messages exist or the poll expires, then return one complete batch.

POST /rpc/sessions/{sessionId}/messages
  Upload one ordered client batch; return transport acknowledgement only.
```

The session-creation response contains transport metadata such as `sessionId`, server generation, negotiated protocol version, poll timeout, and replay limit. It is not a JSON-RPC response.

The upload response contains no JSON-RPC messages and no unrelated events. `204 No Content` means that the complete batch was accepted into the logical session. All server-to-client JSON-RPC traffic—responses, reverse requests, and notifications—arrives only through the receive operation. This produces one authoritative server-to-client order and removes the current race between POST side effects and the event stream.

Each logical frame wraps exactly one JSON-RPC message:

```ts
type SessionFrame = {
  generation: string;
  sequence: number;
  message: JsonRpcMessage;
};
```

Request IDs correlate JSON-RPC requests and responses. Transport sequence numbers order and recover delivery. They must remain separate concepts.

## Required invariants

1. **One session generation.** Every session is bound to an authenticated client and one App Server generation. Frames from an obsolete generation are never dispatched. A server restart produces a new generation and forces fresh authoritative baselines.

2. **One sequencer per direction.** The client and server independently assign contiguous sequence numbers. Responses may complete in a different order than requests, but every outbound response, request, and notification enters the same directional sequencer.

3. **One active receive poll.** The client has at most one active receive request. The server queues frames while no poll is outstanding. Engine.IO makes the same restriction to preserve packet ordering ([Engine.IO polling rules](https://github.com/socketio/engine.io-protocol#http-long-polling)).

4. **One serialized uploader.** Product calls may be concurrent, but their frames enter one client queue and at most one POST is active. If the acknowledgement is lost, the client retries the identical sequence range. The server accepts duplicates without redispatching them and rejects a gap with the expected sequence. BOSH likewise requires request-ID ordering and returns a buffered copy for an exactly repeated request after a broken connection ([XEP-0124 ordering and broken connections](https://xmpp.org/extensions/xep-0124.html#request-ids)).

5. **Receive acknowledgement and replay.** `after=N` means that the client fully applied every server frame through `N`. The server returns only a contiguous range after `N` and retains a bounded replay window. Repeating the same poll after an ambiguous failure may return the same frames; the client ignores already-applied sequence numbers. SSE's `Last-Event-ID` provides a similar reconnect cursor, but the server still has to implement retention and replay ([HTML Server-Sent Events](https://html.spec.whatwg.org/multipage/server-sent-events.html#the-last-event-id-header)).

6. **Explicit replay failure.** If `after` is outside the retained window, the server returns `resyncRequired`; it never silently skips forward. The client obtains fresh scope baselines and reports `resynchronizing`, not `App Server unavailable`.

7. **Bounded state.** Session queues and replay buffers have hard limits. A client that falls behind is moved to explicit resynchronization rather than consuming unbounded memory.

8. **No unsafe mutation replay.** Transport frames can be retried within a live session because sequence deduplication prevents redispatch. After loss of the session or server generation, mutating domain commands are retried only when they carry a durable idempotency key such as `commandId`; otherwise their result is `outcomeUnknown`.

9. **Reverse RPC is ordinary RPC.** A server request travels in the receive poll; the client handler's result travels in the upload queue as a JSON-RPC response. Cancellation is a best-effort protocol notification referencing the request ID. No public `respond()` transport round trip is needed.

10. **Poll completion is healthy.** A normal empty poll timeout is liveness, not disconnection. The client immediately starts the next poll. Only repeated transport failures beyond a retry budget enter `reconnecting`; loss of replay enters `resynchronizing`.

RFC 6202 recommends suppressing caches and choosing poll timeouts with intermediary limits in mind; it notes that roughly 30 seconds is safer than very long waits across heterogeneous infrastructure ([RFC 6202, timeouts and cache control](https://www.rfc-editor.org/rfc/rfc6202.html#section-5.5)). The exact OpenAIDE default should be measured and configurable.

## Why not make streaming fetch the baseline?

A fetch response body is useful for server-to-client streaming, and EventSource defines ordered parsing, automatic reconnection, and `Last-Event-ID` ([HTML Server-Sent Events](https://html.spec.whatwg.org/multipage/server-sent-events.html)). However, partial responses can be buffered by intermediaries, and the HTML specification itself warns that HTTP chunking by an unaware layer can harm event-stream reliability ([HTML event-stream authoring notes](https://html.spec.whatwg.org/multipage/server-sent-events.html#authoring-notes)). A capability-tested fetch/SSE adapter is still worthwhile because it removes repeated poll headers and the short hand-off latency between polls.

A single endless upload fetch is not a browser duplex solution. Fetch currently permits only `duplex: "half"`: the browser sends the complete request before processing its response; `full` remains reserved ([Fetch Standard](https://fetch.spec.whatwg.org/#dom-requestinit-duplex)). Chromium's first-party guidance therefore recommends two associated fetches for duplex communication and notes that streaming request bodies do not work over HTTP/1.x and can be buffered by software or servers ([Chrome request-streaming guidance](https://developer.chrome.com/docs/capabilities/web-apis/fetch-streaming-requests)). OpenAIDE should use ordinary finite POST bodies, which are more compatible.

## Alternatives

| Option | Finding |
| --- | --- |
| **Engine.IO forced to polling** | Technically sound and closest existing implementation: session ID, held GET, short POST, one active request per direction, packet batching, and heartbeat. It guarantees live ordering, but Socket.IO documents at-most-once delivery by default and requires application-level IDs, retained events, and reconnect offsets for missed messages ([delivery guarantees](https://socket.io/docs/v4/delivery-guarantees/)). Its optional recovery can fail and still requires full state synchronization ([connection recovery](https://socket.io/docs/v4/connection-state-recovery/)). OpenAIDE needs its own replay and baseline rules either way. |
| **Full BOSH** | Strong reliability precedent, but its XML wrapper and XMPP-oriented session protocol are unnecessary. Borrow RID/ack/retry invariants rather than adopting BOSH wire syntax ([XEP-0124](https://xmpp.org/extensions/xep-0124.html)). |
| **Bayeux/CometD** | Proven two-HTTP-connection, long-poll pub/sub protocol with handshake and reconnect advice, but its public/subscribe model is a poorer fit for typed bidirectional JSON-RPC ([Bayeux specification](https://docs.cometd.org/current8/reference/#_bayeux)). |
| **gRPC-Web** | The official browser implementation supports unary and server-streaming RPCs, not client or bidirectional streaming, and normally adds a gateway proxy ([gRPC-Web](https://github.com/grpc/grpc-web#streaming-support)). |
| **WebTransport** | Supplies real bidirectional streams, but the current W3C document remains a Working Draft and its HTTP/2 and HTTP/3 mappings are still described as work in progress ([W3C WebTransport](https://www.w3.org/TR/webtransport/)). It is not the compatibility fallback for a restrictive network. |
| **WebRTC DataChannel** | Can be reliable and ordered, but uses SCTP over DTLS over ICE/UDP and brings peer negotiation and NAT traversal machinery that browser–server RPC does not need ([RFC 8831](https://www.rfc-editor.org/rfc/rfc8831.html)). |

## Engine.IO versus a small OpenAIDE adapter

Engine.IO forced to polling is viable, but a small OpenAIDE adapter is the better immediate integration fit:

- Engine.IO's default delivery semantics do not remove OpenAIDE's need for sequence replay, idempotency, task revisions, and baseline recovery.
- Its Rust implementation, `engineioxide`, integrates as a Tower/Hyper service ([crate documentation](https://docs.rs/engineioxide/latest/engineioxide/)). The current App Server instead has a small raw-`TcpListener` HTTP edge and no Tower/Hyper dependency ([App Server Cargo manifest](../../openaide-rs/app-server/Cargo.toml), [current listener](../../openaide-rs/app-server/src/protocol_edge/local_http/listener.rs)). Adopting Engine.IO proper would therefore also be an HTTP-server-stack migration.
- The current listener already supports a dedicated event-stream connection, heartbeats, and concurrent accepted connections. Replacing its mixed response side effects with a session queue and adding complete long-poll receive responses is narrower than importing Engine.IO's upgrade, binary-packet, and framing machinery ([current HTTP writer](../../openaide-rs/app-server/src/protocol_edge/local_http/listener/http.rs)).

The transport seam should nevertheless make an Engine.IO, streaming-fetch, WebSocket, IDE IPC, or future WebTransport adapter possible without changing `RpcPeer` or product code.

## Recommended next design step

Specify the session state machine and conformance tests before implementation. The same tests should run against an in-memory adapter and the HTTP long-poll adapter, covering duplicate upload, lost upload acknowledgement, lost poll response, reordered network completion, poll timeout, replay, replay-window expiry, old-generation frames, server restart, reverse requests, cancellation, and two-client permission races.
