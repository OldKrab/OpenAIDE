# P403 remaining architecture gap audit

## Result

Selected `P404-delete-dead-shell-settings-snapshot-contract` as the next cleanup packet.

## Findings

- Shared Frontend no longer imports or stores the transitional shell `SettingsSnapshot`.
- `packages/app-shell-contracts/src/webview/settings.ts` still exports that unused full shell snapshot type.
- The MCP and Skills Settings panel components are now unreachable because Settings rendering no longer receives a full shell snapshot.
- VS Code still has shell-private skill scanning types, so the next packet should delete only the unused full snapshot/common settings pieces and unreachable Frontend panels, not the still-used skill record helpers.

## Next

Delete the dead shell snapshot contract surface and unreachable panels, then keep the larger App Server-owned MCP/Skills Settings projections as future product work.
