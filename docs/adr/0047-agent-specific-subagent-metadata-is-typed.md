# Agent-Specific Subagent Metadata Is Typed

Status: accepted

Native Subagent lifecycle and product invariants remain provider-neutral, while an Agent integration extension may translate legacy events and map recognized useful, production-safe metadata into typed normalized Subagent details. Codex may therefore enrich presentation with known fields such as native path, role, model, reasoning effort, or activity kind without exposing raw `_meta` to Frontend or making it a lifecycle dependency. Unknown safe metadata remains limited to redacted diagnostics, and content, secrets, unstable blobs, duplicates, or fields without defined product meaning are not persisted or rendered.

Agent navigator rows stay concise with name, delegated task, nesting, lifecycle, and attention state. Additional recognized Agent-specific fields appear in a compact Details disclosure within the selected history header rather than making every selector row provider-specific.
