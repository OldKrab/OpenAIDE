# Live ACP Turn Bridge

OpenAIDE will treat real ACP support as a live bidirectional turn bridge, not a batch-only adapter. ACP agents can call client methods such as `session/request_permission` while `session/prompt` is still running, so the runtime must keep the turn open, normalize visible updates as they arrive, and route user decisions back to the agent; auto-denying or buffering everything until turn completion would make agents such as `codex-acp` unreliable and would not satisfy the product permission model.

OpenAIDE will run one ACP agent process per configured Agent identity, with Tasks mapped to ACP sessions inside that process. This follows ACP's session model, keeps session list/load/config behavior at the Agent boundary, and avoids mixing different Agent configurations in one global process while still avoiding a separate process for every Task.

OpenAIDE will make the runtime-to-host connection bidirectional JSON-RPC. The Rust runtime owns ACP orchestration, task state, persistence, and normalization, while each frontend host implements host-only client capabilities such as editor-buffer filesystem access, terminals, secret-backed auth, elicitation UI, and user notifications. The VS Code extension is the first host implementation; the same runtime boundary must also support a future desktop app.

Live ACP updates flow from runtime to Host as ordered JSON-RPC notifications or deltas. `session.prompt` must not be the only way the UI learns about chat, tools, permissions, terminals, or interruptions; final snapshots remain useful for reconciliation, but live interaction depends on notifications.

Runtime persists normalized ACP updates before emitting Host/webview notifications. This keeps passive restore and crash recovery aligned with what the user saw, even if it adds a small amount of streaming latency.

ACP agent message chunks are fragments, not complete assistant messages. The runtime coalesces contiguous agent text chunks into one normalized Chat message; tool calls, permission requests, configuration updates, thoughts/activity, interruptions, and other non-text updates close the current text run.

OpenAIDE advertises ACP client capabilities only when the current Host can actually satisfy them. Planned support must not be advertised to Agents.

Filesystem capabilities are satisfied through the Host file bridge. The runtime must route ACP file reads and writes to the current Host so editor buffers, execution roots, and write conflicts are handled consistently across VS Code and future desktop Hosts.

Terminal capabilities are also satisfied through a Host bridge. The runtime coordinates ACP terminal lifecycle and persistence, while the Host owns process execution, output capture, and cancellation semantics for the current shell environment.
