# P31 Next Slice Selection

Completed: 2026-06-27T03:07:57+03:00

## Selected Slice

Split ACP update projection responsibilities out of the oversized
`agent/acp_update_projection.rs` module.

## Rationale

The remaining oversized backend files are concentrated in ACP runtime/session and ACP
projection/content helpers. `acp_update_projection.rs` is a good next slice because
it already exposes separate projection concepts with separate callers:

- live prompt event and permission projection;
- replayed session history projection;
- prepared options session catalog projection;
- active session config update projection;
- ACP config-option normalization.

Those concepts are related by ACP update normalization, but their lifecycles and
callers are different. Splitting them first reduces shared surface before larger
runtime/session ownership work.

## Non-Selection

Do not split `AcpRuntimeKernel` yet in this slice. It still combines registry access,
options-session reuse, live-session registry, and probe/auth helpers. Those boundaries
need a separate API contract after projection helpers are no longer bundled together.

Do not change prompt content behavior or Agent session lifecycle in this slice.
