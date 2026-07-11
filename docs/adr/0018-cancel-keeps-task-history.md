# Cancel Keeps Task History

When the user stops a Running Task, OpenAIDE will call ACP `session/cancel`, answer pending permission and elicitation waits as canceled where the protocol requires it, stop or release active Host terminals as needed, and keep the Task, Chat history, and already persisted activity. Canceling a turn is not a local-only UI stop and is not a hard kill of the Agent process unless the Agent or transport has become unrecoverable.
