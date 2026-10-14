# Project files rework

Status: accepted. Follows [ADR-0066](../adr/0066-project-files-belong-to-a-workspace-and-open-beside-chat.md) and the reviewed Task review panel prototype. It supersedes the presentation described in the [production integration plan](project-file-access.md); that plan's App Server boundary, limits, and verification record stand until a slice below changes them.

## Slices

Each slice ships on its own and leaves the product usable.

1. **Task review panel.** Replace the full-page destination with the panel beside Chat, fixed to the Task Workspace. Split Changes and Files into separate views. Changes gets the tree/list switch, per-file status and line counts, and the one-page or one-file choice. Move line selection, the comment box, and notes into the shared reader; remove the floating selection toolbar and the Select range control. Size the line-number column to its digits. Refresh on open, on turn end, and on request. Phone layout, including the top bar entry with its count and the overflow menu. Uncommitted changes only.
2. **Project viewer and Task-free access.** Make discovery, search, changes, and diff take a Project and a workspace. Add "Browse files" to the Project menu and the full-page viewer with "Ask in a new Task". Add the workspace selector and the suggestion to both entries, the per-Task remembered selection, and the folder-qualified comment path.
3. **Default-branch base and change count.** Add the "since the default branch" base and its merge-base resolution. Show the change count in the Task header and keep it in step with the selected workspace.
4. **Search palette.** Move find-by-name and content search into the palette; full screen on a phone.

Later, not planned here: changes per turn, and choosing an arbitrary base.

## App Server boundary

- Slice 1 adds added and removed line counts to the change list and needs no new authority.
- Slice 2 changes the identity the read-only methods accept from a Task to a Project plus workspace. App Server lists the selectable workspaces and rejects one that does not belong to the Project's repository. The existing containment, symlink, limit, and ignore rules apply unchanged to the selected root. Protocol types change; regenerate and check the TypeScript bindings.
- Slice 3 adds the base to the changes and diff requests and reports which commit was used, or that none could be found.
- Git stays read-only, without fetch, external diff drivers, or text conversion. Work runs outside the shared protocol lock with the existing bounds.

## Diagnostics

Every new or changed operation emits structured start and terminal events with a stable operation name, safe correlation identifiers, outcome, duration, and a classified error. The turn-end refresh logs its trigger and result once per turn, not per poll. Workspace and base resolution log the kind of result (Task Workspace, selected worktree, remote or local default branch, none) without paths or branch names.

## Verification

- App Server: workspace authorization for a foreign worktree, a non-Git Project, a detached worktree, a missing default branch, merge-base resolution with and without a remote-tracking branch, and line counts for added, deleted, renamed, binary, and conflicted files.
- Frontend: view and tab state kept separately for Changes and Files, both diff modes, range selection by pointer, touch, and keyboard, note add, edit, and remove against a Composer draft the user has also typed in, refresh on turn end without losing scroll position or an open comment box, and stale responses discarded on Task, workspace, and base change.
- Layout: wide and phone widths in both themes, the resized and maximized panel, the phone top bar with every optional control present, and the unchanged VS Code native-file path.

## Prototype

The contract is the `task-review-panel` prototype as approved. It used fixture data and stand-ins for Subagents, jump to a message, permissions, and Plan. Two things it did not exercise: the turn-end refresh, and remembering layout choices across reloads.

## As built

All four slices are implemented. Where the result differs from the prototype or leaves something open:

- The panel starts at 58% of the Task Page instead of half, because the list column and a diff do not both fit in half of a 1280-pixel window. The list column narrows with the panel, not with the window.
- A phone shows one opened file at a time; File Tabs appear from the wide layout up.
- The Project viewer is presentation state of the App Shell, not a route: a reload returns to the surface underneath. Giving it a route needs a shell navigation contract and is marked in the code.
- "Ask in a new Task" starts the new Task with one quote or comment. Collecting several before starting has no Composer to hold them and is not built.
- The workspace suggestion reads the paths of tool steps already in Chat. A shell command that reports no location does not produce a suggestion, which is why it is only ever offered.
- A view kept behind the other one (the tree behind Changes, read diffs behind Files) stays mounted so its folders, scroll position, and comments in progress survive the switch.
