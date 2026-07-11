# Task Delete Deletes Native Session When Supported

When a user confirms destructive deletion of a local Task bound to a Native Session, OpenAIDE will call ACP `session/delete` if the Agent advertises that capability. If native deletion is unavailable or fails after local deletion is committed, OpenAIDE keeps a local tombstone so the same external Native Session is not silently re-adopted; `session/close` remains a resource cleanup operation, not a substitute for deletion.
