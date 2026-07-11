# Capability-Gated Prompt Content

OpenAIDE will turn composer text and attachments into ACP prompt content according to the selected Agent's advertised prompt capabilities. Text is always supported; embedded resources, images, and audio are used only when advertised, otherwise OpenAIDE downgrades to resource links when the Agent can access the resource or blocks the prompt with a clear capability error rather than sending unsupported content and relying on Agent failure.
