# Subagents Are Task Children

Status: accepted

OpenAIDE represents an Agent-created Subagent as a restricted child of the current Task, with its own inspectable history and optional nested Subagents. A Subagent is neither a separate Task nor only a flattened activity record: making it a Task would falsely imply independent user ownership and controls, while flattening it would discard the child history exposed by native ACP subagent sessions. The first product slice is inspection-only; OpenAIDE exposes child actions only when ACP positively advertises the corresponding capability.
