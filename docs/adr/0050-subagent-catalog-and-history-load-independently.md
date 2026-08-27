# Subagent Catalog and History Load Independently

Status: accepted

Ordinary Task snapshots carry only a lightweight Subagent overview with total, running, failed, and native-inspection availability. Opening the Agent navigator loads a dedicated paged and subscribed catalog containing the complete spawn-ordered hierarchy and later lifecycle changes, while selecting one Subagent loads its independently paged normalized history. This keeps passive Task open bounded without making older Subagents unreachable.

The selected history distinguishes a valid running child waiting for first activity, a disconnected child whose outcome is unknown, and locally unavailable history whose separate journal cannot be read. History unavailability does not quarantine the Task; Reload from Agent is offered only when authoritative load is supported.
