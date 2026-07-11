# External Adoption Requires Load

OpenAIDE will require ACP `session/load` for external Native Session adoption in the first iteration. `session/load` replays conversation history, which lets OpenAIDE create a Task with truthful Chat content; `session/resume` does not replay history and is reserved for reconnecting a Native Session already represented by local Task state.

This does not make local Native Session bindings durable across an OpenAIDE runtime restart in the first iteration. Until restart-safe `session/resume` is implemented and tested for the selected Agent, OpenAIDE clears stored `agent_session_id` bindings during shutdown/recovery and starts a fresh Native Session for follow-up work. This keeps Task Chat truthful and avoids silently routing a Task to an Agent session the runtime can no longer prove is live.
