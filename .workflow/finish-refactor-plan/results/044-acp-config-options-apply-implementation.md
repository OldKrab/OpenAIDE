# P23 ACP Config Options Apply Implementation

Completed: 2026-06-27T02:54:16+03:00

## Implemented

- Added `agent/acp_config_options_apply.rs`.
- Moved `apply_config_options`, `set_prepared_config_option_after_prior_updates`,
  `PreparedOptionsSetContext`, and private config-option apply helpers out of
  `agent/acp_runtime_kernel.rs`.
- Updated prepared options session and ACP session worker imports to use the new
  module.
- Kept `AcpRuntimeKernel` as owner of process/session registries, options session
  lifetime, active ACP sessions, and lifecycle operations.

## Tests Added Or Updated

- No tests were deleted or intentionally weakened.
- Existing ACP config-option tests continue to cover option application and update
  ordering through the moved module.
