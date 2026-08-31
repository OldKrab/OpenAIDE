# Subagent Histories Have Independent Ordering

Status: accepted

OpenAIDE preserves an independent durable sequence for Main Agent Chat and every Subagent History because ACP guarantees transport order only within each session and defines no chronology across concurrent sessions. Task Chat orders spawn and terminal lifecycle updates within the Main Agent stream, and the Subagent catalog preserves hierarchical spawn order; timestamps never manufacture an exact cross-history order.

While a Task is open, Frontend subscribes continuously to the lightweight Subagent catalog but receives detailed live history updates only for the selected Main Agent or Subagent. Switching selection replaces that detail subscription, while catalog activity revisions drive ephemeral unseen-activity indicators for unselected histories.
