# Resolved MCP Servers for Task Sessions

OpenAIDE will resolve MCP Servers from Settings using global/workspace precedence and pass every enabled server compatible with the selected Agent's ACP MCP transport capabilities to real Task Native Sessions. MCP Servers are not selected per Agent or per Task in the first iteration; this keeps the composer focused on starting work, makes MCP setup visible in Settings, and still protects session start through validation and capability filtering.

Task Native Sessions receive resolved MCP Servers during session setup. Configuration Options are discovered from that same Task Native Session rather than through a separate options-only session.
