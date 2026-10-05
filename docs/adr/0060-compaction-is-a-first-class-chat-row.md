# Context compaction is a first-class Chat row

OpenAIDE advertises ACP `session.compaction` to every Agent and projects compaction updates into a durable Compaction Chat row keyed by the Agent's `compactionId`, rather than leaving adapters to fall back. Without the capability, adapters such as `claude-agent-acp` replay the model-facing continuation summary as User text and show a legacy tool call live, so a reloaded Task presents text the user never wrote.

One projection serves live updates and `session/load` replay, so reload matches the live view. Status is stored as the Agent reported it, with unrecognized values kept as unknown instead of being mapped to a lifecycle. The summary is collapsed by default in Frontend because it is model-facing.

Existing Tasks are not migrated: their history is replaced from the Agent on reload. No text-based filter recognizes summaries from agents that do not support the capability, because guessing at free-form content would hide real User messages.
