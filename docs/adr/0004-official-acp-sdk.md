# Official ACP SDK

OpenAIDE will use the official Rust `agent-client-protocol` SDK for ACP protocol types and agent transport instead of maintaining hand-written JSON schemas. OpenAIDE is ACP-only for the current product direction; Agent definitions identify and launch ACP workers, while auth methods and session configuration are discovered through ACP at runtime. The OpenAIDE Host Capability RPC remains a separate, frontend-neutral boundary so the shared runtime can serve both the VS Code Host and a future desktop Host while staying aligned with upstream ACP stable and draft surfaces.

Session Config Options are the primary configuration path. Legacy ACP `session/set_mode` and `session/set_model` are implemented only as fallback compatibility when an Agent does not provide equivalent `configOptions`; OpenAIDE must not show duplicate model/mode controls when config options already cover them.

OpenAIDE will enable the SDK's unstable feature set when needed for RFD/draft surfaces. Draft support remains capability-checked and protocol-version checked; enabling SDK types does not mean treating unsupported Agent capabilities as available.
