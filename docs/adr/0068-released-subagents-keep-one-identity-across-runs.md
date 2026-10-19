# Released-Form Subagents Keep One Identity Across Runs

Status: accepted; amends [ADR-0042](0042-app-server-owns-subagent-identity.md)

In the released ACP Subagents form, a child session is a reusable worker: `subagent_update` moves it from `idle` back to `running` when its parent messages it again, and each run's prompt arrives on the child as a `session_message`. OpenAIDE therefore keeps one Subagent Identity per announced child session across runs. A new run returns the same Subagent to Running and appends to the same Subagent History; it does not create a new lifecycle generation. The draft form keeps ADR-0042's rule that each newly announced generation receives a new identity, because the draft announces every run as a new spawn.

The released form has no spawn event that carries the delegated prompt, and the Agent already records its own delegation as ordinary Tool calls, so Main Agent Chat shows those Tool rows and gets no separate delegation row. The Agent's `title` becomes the Subagent's name and `description` its delegated task. Both follow ACP patch semantics: an omitted field leaves the value unchanged and `null` clears it.

`idle` with `end_turn` or no stop reason is Completed, `cancelled` is Cancelled, any other stop reason is Failed, and `unknown` is Disconnected. A Subagent that advertises `cancel` can be stopped on its own with `session/cancel` addressed to its child session. App Server never assumes the outcome; the status changes only when the Agent reports it.
