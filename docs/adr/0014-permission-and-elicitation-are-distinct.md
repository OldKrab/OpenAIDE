# Permission and Elicitation Are Distinct

OpenAIDE will model ACP permission requests and elicitation requests as different user-facing interactions. `session/request_permission` asks the user to approve, reject, or cancel a proposed Agent action and remains as an auditable permission block; draft elicitation asks the user for structured information and must not be rendered as an approval card or flattened into normal Chat unless the capability is unavailable and the fallback is explicit.
