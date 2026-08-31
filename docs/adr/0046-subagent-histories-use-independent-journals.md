# Subagent Histories Use Independent Journals

Status: accepted

Each Subagent History is stored under its owning Task in an independently recoverable snapshot and journal pair, while the Task projection owns the Subagent summary tree, lifecycle, and committed history sequence. Spawn is committed before child output and terminal lifecycle after child output, preserving the ACP ordering contract without mixing every history into the Main Agent's Chat vector or coordinating unrelated child files in one rewrite. A missing or damaged child history reports that history as unavailable without quarantining the Task and Main Agent Chat.

Ordinary Task snapshots include the bounded Subagent summary tree but do not embed complete child histories. Selecting a Subagent loads its normalized history through an independently paged interface using the same message presentation shape as Chat.
