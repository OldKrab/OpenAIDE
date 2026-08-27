# App Server Owns Subagent Identity

Status: accepted

App Server assigns and persists the stable product identity of each Subagent instance, while ACP child session identifiers remain private correlation data at the Agent integration seam. This prevents a draft, connection-scoped protocol identifier from becoming a Frontend or persistence contract and lets live activity and replay reconcile through one product-owned identity model. Replaying an already recorded spawn reconciles to its existing Subagent Identity, while every newly announced lifecycle generation receives a new identity even when its display name repeats; OpenAIDE does not infer a portable logical-worker identity that ACP did not provide.
