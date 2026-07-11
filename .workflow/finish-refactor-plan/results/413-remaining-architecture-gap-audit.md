# P413 remaining architecture gap audit

## Result

Selected `P414-non-agent-settings-projections-api` as the next packet.

## Findings

- A0-A9 are marked complete in workflow state and refactor plan status.
- Recent scans show the clearest remaining product gap is App Server-owned non-Agent Settings sections.
- Frontend now hides MCP/Skills because no App Server-owned projections exist.
- App Server Settings snapshots now advertise only renderable `agents` and `commonSettings`.
- Historical `Proposed next slice` entries remain in the plan, but most are completed refactor notes rather than current top-level blockers.

## Next

Design the App Server-owned MCP/Skills Settings projection API before implementation. This is design-sensitive because it decides the Backend/Frontend seam, replaces shell-owned scan behavior, and affects UI section availability.
