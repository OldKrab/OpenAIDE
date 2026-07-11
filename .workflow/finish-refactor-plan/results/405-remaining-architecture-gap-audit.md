# P405 remaining architecture gap audit

## Result

Selected `P406-hide-unavailable-settings-sections` as the next packet.

## Finding

After removing the full shell Settings snapshot and unreachable MCP/Skills panel components, `SettingsView` still exposes MCP and Skills tabs that render a skeleton even when no loading work is happening. That is not a responsive or honest UI state.

## Next

Hide unavailable Settings sections until App Server-owned MCP/Skills projections exist. Keep `SettingsTabId` protocol compatibility intact; only the current renderable tabs should change.
