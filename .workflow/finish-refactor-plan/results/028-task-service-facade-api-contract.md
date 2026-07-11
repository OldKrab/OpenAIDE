# P07 Task Service Facade Split API Contract Draft

Drafted: 2026-06-26T22:40:16+03:00

## Selected Slice

Extract read-only Task queries and non-turn Task commands from `TaskService` while
keeping `TaskService` as the external facade.

## Proposed Shape

`TaskService` remains the public App Server service used by `Runtime` and transport
dispatch for this slice. It owns construction and composes narrower internal modules.
It should not keep accumulating workflow logic.

Internal modules:

- `TaskQueries`
  - owns read-only operations over Task storage and snapshots;
  - holds `Store`, shared mutation/read lock, and `TaskMutations` only as needed for
    current revision;
  - never mutates product state, starts Agent work, or publishes notifications.
- `TaskCommands`
  - owns non-turn Task mutations that do not start, resume, prompt, cancel, or close
    Agent sessions;
  - uses `TaskMutations` for all durable Task writes;
  - may call `AgentGateway` only for post-commit native-session delete side effects
    for historical delete, using committed Task facts.
- `TaskTurnLifecycle`
  - remains the owner of create, prompt, cancel, permission response, shutdown turn
    cleanup, and volatile runtime recovery.
- `TaskService`
  - becomes a thin facade that constructs and delegates to those modules;
  - may keep Agent probe/auth/list/config methods temporarily until a separate Agent
    service slice is accepted.

## API Methods

`TaskQueries`:

```rust
impl TaskQueries {
    pub(crate) fn new(store: Store, store_update_lock: Arc<Mutex<()>>, mutations: TaskMutations) -> Self;
    pub(crate) fn list(&self, params: TaskListParams) -> Result<TaskListResult, RuntimeError>;
    pub(crate) fn diagnostics(&self) -> Result<TaskDiagnostics, RuntimeError>;
    pub(crate) fn snapshot(&self, params: TaskSnapshotParams) -> Result<TaskSnapshot, RuntimeError>;
    pub(crate) fn tail(&self, params: ChatTailParams) -> Result<MessagePage, RuntimeError>;
    pub(crate) fn page(&self, params: ChatPageParams) -> Result<MessagePage, RuntimeError>;
    pub(crate) fn tool_detail(&self, params: ToolDetailParams) -> Result<ActivityToolDetails, RuntimeError>;
}
```

`TaskCommands`:

```rust
impl TaskCommands {
    pub(crate) fn new(mutations: TaskMutations, agent_gateway: AgentGateway) -> Self;
    pub(crate) fn mark_read(&self, params: TaskIdParams) -> Result<TaskSnapshot, RuntimeError>;
    pub(crate) fn delete(&self, params: TaskDeleteParams) -> Result<Value, RuntimeError>;
}
```

`TaskService` delegates without changing its public method list in this slice:

```rust
list -> TaskQueries::list
diagnostics -> TaskQueries::diagnostics
snapshot -> TaskQueries::snapshot
tail -> TaskQueries::tail
page -> TaskQueries::page
tool_detail -> TaskQueries::tool_detail
mark_read -> TaskCommands::mark_read
delete -> TaskCommands::delete
create/prompt/cancel/respond_permission/shutdown/recovery -> TaskTurnLifecycle
Agent probe/auth/list/config -> remain in TaskService until Agent service slice
```

## Invariants

- `TaskQueries` is read-only. Static tests should reject use of
  `commit_existing_task`, `create_task`, `write_task`, direct message write helpers,
  `RuntimeNotifier`, `AgentGateway`, or `TurnRunner` from `TaskQueries`.
- `TaskCommands` must use `TaskMutations` for durable writes and must not call
  `Store::write_task`, direct message persistence helpers, or `RuntimeNotifier`.
- Historical delete native-session cleanup must run only after a committed tombstone
  outcome and must use the committed Task record from `TaskCommitFacts`.
- No protocol behavior changes: existing runtime contract tests remain authoritative.
- Changed production files must remain under the source-size limit.

## Required Tests

- Existing runtime contract tests for list, diagnostics, snapshot, chat paging,
  mark-read, archive/restore/delete, and native-session delete still pass.
- Add focused static boundary tests proving `TaskQueries` remains read-only and
  `TaskCommands` does not bypass `TaskMutations`.
- Add or preserve tests proving no-op mark-read/delete outcomes do not advance revision
  or publish notifications.

## Next

Review this contract, then implement this slice only after it is accepted or revised.
