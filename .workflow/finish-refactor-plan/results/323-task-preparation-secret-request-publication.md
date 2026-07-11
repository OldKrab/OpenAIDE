# P323 Task Preparation Secret Request Publication

## Scope

Added the App Server integration point that lets task preparation publish a
pending task-scoped `secret/read` request before waiting for the shell response.

## Decisions

- Task preparation uses typed `ServerRequestRuntime` helpers instead of raw
  method strings or untyped payload construction.
- Pending request visibility is live runtime state; Task storage is not changed
  just to show a pending request.
- Product workflows publish the current Task through `TaskMutations` so direct
  task notification access stays centralized in the mutation commit module.
- This slice stops short of ACP `secret_env` migration. The next slice should
  inject this helper into Agent startup so secret lookup leaves the legacy host
  bridge.

## Verification

- `cargo fmt --all --check`
- `cargo test -p openaide-runtime preparation_secret_request -- --nocapture`
- `cargo test -p openaide-runtime migrated_service_paths_have_no_direct_task_updated_calls -- --nocapture`
- `cargo test -p openaide-runtime server_requests::runtime -- --nocapture`

## Next

Wire ACP Agent `secret_env` lookup through task preparation using the typed
task-scoped `secret/read` request path.
