# P21 Next Slice Selection

Completed: 2026-06-27T02:47:20+03:00

## Selected Slice

Extract ACP prepared config-option application helpers out of
`agent/acp_runtime_kernel.rs`.

## Why This Slice

- `agent/acp_runtime_kernel.rs` is the largest remaining production file and mixes
  runtime ownership with config-option application details.
- The config-option helper block is already cohesive: it parses selected config
  options, sends set-option requests, and drains prior ACP updates while preserving the
  latest catalog.
- Moving this block is a low-risk boundary improvement before deeper ACP runtime
  refactors.

## Scope

- Move prepared config-option apply helpers to a focused Agent module.
- Keep public function names and call sites stable where practical.
- Keep `AcpRuntimeKernel` as owner of Agent registry, process/session registries,
  options session lifetime, and active session lifecycle.
- Preserve all ACP protocol behavior and existing tests.

## Main Risk

The helper code is async and update-order sensitive. The implementation must not change
the order in which prior session updates are drained before set-option responses are
accepted.
