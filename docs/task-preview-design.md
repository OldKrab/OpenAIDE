# Task Preview Design

Status: accepted implementation design

This note records the implemented Task Preview design and its worktree-specific context.

## Agreed decisions

### Product problem

Task Preview exists to help a user verify a Task's identity and context when compact rows are ambiguous or truncated, without opening the Task. Preview must therefore remain passive: showing it must not call `task/open`, restore a Native Session, fetch or synchronize Chat, acknowledge attention, or otherwise mutate Task state.

The preview is not justified as a second presentation of runtime state already visible in the Task row. If a first slice cannot expose enough reliable context to disambiguate Tasks, it should be deferred rather than filled with duplicated row information.

### Approved Task Navigation behavior

- The production implementation follows `packages/frontend/prototypes/sidebar-worktree-context`, variant A.
- Every Task remains a single compact row on desktop and mobile. Titles truncate rather than wrap.
- Only Tasks backed by linked worktrees receive a quiet Git worktree icon in the trailing status area. Project-root Tasks receive no additional marker. The icon has an accessible label and tooltip and does not replace runtime status.
- On pointer devices, dwelling on a Task for 550 ms opens a passive rich preview. While any preview is open, moving directly to another Task updates it immediately.
- The Task row and preview form one hover region. Moving from the row into the preview keeps it open; leaving both closes it after a short grace period. Keyboard focus opens immediately, Escape and outside click dismiss, and opening the Task clears pending timers.
- The preview shows Task identity, Project, Task Workspace, branch or detached revision, runtime state, and useful actions supported by authoritative summary data. Hover never opens the Task, restores a Native Session, fetches Chat, or acknowledges attention.
- Mobile does not use hover preview. The existing Task `…` action opens a compact action sheet. **Task details** reveals Project, Task Workspace or Project root, branch when present, and status; Back returns to actions, and backdrop, close, or Escape dismisses it.

### Unavailable Task Workspace presentation

- The production implementation follows `packages/frontend/prototypes/unavailable-task-context` for worktree-unavailable and Project-unavailable states.
- Workspace availability is independent of Task runtime status. A missing worktree or Project never marks an otherwise idle Task as failed in Task Navigation or the Task header.
- Saved Task history remains readable. The composer is visible but cannot edit or Send while its Task Workspace is unavailable.
- A missing worktree presents Refresh and **Manage worktrees** recovery. A missing Project presents **Project settings** and **Reconnect folder** recovery.
- Desktop uses one quiet inline notice below the Task header. Mobile keeps the Task header to one line and places the same notice and actions below it without horizontal overflow.

## Evidence and constraints

- Opening an existing Task is the sole automatic Native Session recovery and history-synchronization trigger.
- The current App Server Protocol `TaskSummary` carries Task, Project, and Agent identities; title; status; update and activity timestamps; unread and attention state; and whether the Task has messages.
- Project display text is joined from the Project collection in Frontend.
- The legacy Frontend `TaskSummary` shape also contains creation time, isolation, and workspace root, but the current protocol mapping substitutes update time, `local`, and an empty workspace root. Those substituted values are not reliable preview information.
