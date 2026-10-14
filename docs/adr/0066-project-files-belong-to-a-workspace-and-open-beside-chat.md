# Project Files Belong to a Workspace and Open Beside Chat

Status: accepted

Project files was built as a Task Page destination that hides Chat. To read a file or check a diff the user had to open or create a Task, then leave the conversation, and a comment sent them back to it. The changed files were a flat list without sizes, and only uncommitted changes could be reviewed. Two needs went unmet: reading a Project's files without a Task, and reviewing what an Agent changed while still talking to it.

This replaces the layout decision of [ADR-0036](0036-file-viewer-task-page-layout.md), the line action of [ADR-0034](0034-file-lines-become-composer-quotes.md), and the presentation paragraphs of [ADR-0035](0035-file-viewer-snapshots-and-shell-boundary.md). The snapshot, handle, download, and path-authority decisions of ADRs 0035, 0037, and 0038 stand. The reviewed prototype is the visual and interaction contract; the [rework plan](../plans/project-files-rework.md) orders the work.

## Choice

**Files and changes belong to a workspace.** A workspace is a Project root or one of the worktrees of that Project's repository. Project files shows what is in the workspace and how it differs from a base. It never claims that a Task made a change: an Agent edits mostly through shell commands, several Tasks can share a workspace, and one Task can work in more than one. Labels name the workspace ("changes in retry-cancel"), never the Task. Everything is read-only. VS Code keeps its native files and editor.

**There are two ways in, and they differ in what they can do.**

- From a Project: "Browse files" in the Project's menu opens a full page with no Chat and no Composer. It opens on the Project root. Selected code offers "Ask in a new Task".
- From a Task: an entry in the Task header shows the number of changed files and opens a panel beside Chat, with a divider to resize it and a control to maximize it. It opens on Changes in the Task Workspace. Comments go to that Task's Composer.

**The workspace can be changed, and is only ever suggested.** Both entries have a workspace selector limited to the Project root and its repository's worktrees. From a Task the default is always the Task Workspace. When the Agent is working in another worktree, a dismissible line offers to switch; the panel never switches by itself. The selection is remembered per Task, and the change count in the header follows it. A comment made in a workspace other than the Task's names that workspace's folder in its path.

**Changes and Files are separate views.** A switch at the top of the panel moves between them, and each keeps its own state.

- Changes lists only changed files, as a folder tree that can be switched to a flat list, each with its status and lines added and removed. It compares against one of two bases: uncommitted work, or everything since the workspace's common commit with the default branch. On the default branch only uncommitted work is offered. The diffs show either all on one page or one file at a time; the user chooses and the choice is remembered. Changes has no File Tabs.
- Files shows the whole tree and the opened files, with File Tabs on a wide window. Changed files carry their counts in the tree.
- The two are linked only by explicit actions: "Open in Files" on a diff and "Show diff" on a file.

**A comment is written where the code is.** Selecting a line opens a comment box directly under it; selecting another line extends the range. Nothing floats over the code. Adding the comment appends a File Quote followed by the comment text to the Composer, and leaves a note under the lines. The panel stays open, so comments accumulate. A note can be edited or removed, which rewrites or removes its text in the Composer. A quote added without a comment is shown as a small marker, not as a comment.

**Refresh is automatic at the moments that matter.** Changes and the count refresh when the panel opens, when the Agent finishes a turn, and on request. The list of worktrees refreshes at the same moments.

**A phone gets its own layout** below the 760px breakpoint: list and reader are separate screens, Changes and Files are the main tabs, the base switch sits below them, and the workspace is a small chip that opens a bottom sheet. In the phone top bar the entry is an icon with a count. Subagents, jump to a message, and permissions move into one overflow menu so the Task title keeps its room; the overflow shows a mark when something inside needs attention.

Finding a file by name and searching file contents open from one search control as a palette, full screen on a phone.

A Project that is not a Git repository has Files and search, and no Changes.

## Mechanism

- App Server owns workspace resolution, the list of selectable workspaces, the base commit, the change list with line counts, and the diffs. Frontend owns the open view, the tree/list and page/file choices (remembered per device), open tabs, and notes not yet sent.
- The base for "since the default branch" is the merge base with the remote-tracking default branch when one exists, otherwise the local one. Nothing is fetched.
- File access stops requiring a Task: discovery, search, changes, and diff take a Project and a workspace, and App Server checks the workspace belongs to that Project. Handles stay bound to the client as today. This amends [ADR-0038](0038-file-viewer-app-server-methods.md) when that slice lands.
- The turn-end refresh is driven by the existing Task change event. No file watcher and no per-Task snapshot is added.
- A note is Frontend state until the message is sent. If the user has changed a note's text in the Composer by hand, a later edit of the note appends instead of replacing.
- Line selection, the comment box, and notes live in the shared source and diff reader as typed state. The line-number column is sized to its digits.

## Considered Options

- **Attributing changes to a Task** through tool calls, checkpoints, workspace snapshots, or a file watcher. Rejected: recorded Claude sessions edit almost entirely through shell commands, which none of the cheap sources see, and the reliable ones cost a snapshot or a watcher per Task for a claim the user did not ask for.
- **Keeping the full-page destination and adding a tree.** Rejected: it keeps the round trip between code and conversation for every comment.
- **One tree for Files with a "changed only" filter.** Rejected after the prototype: reviewing changes and browsing a project are different jobs, and one list with a filter made each harder to read.
- **A new entry in Task Navigation for files.** Rejected: it adds a permanent control for something reached from a Project's menu or a Task's header.
- **Switching to the Agent's worktree automatically.** Rejected: the panel would change under the user. The Task Workspace is the default and the other worktree is offered.
- **Copying the VS Code source-control layout.** Rejected as a whole: staging, commit, and editor concepts do not apply to a read-only review surface. The tree of changes and per-file counts were taken because they fit.
- **A bar docked to the bottom of the phone screen for the selection.** Rejected after the prototype: it offered no place to write and read as a toolbar unrelated to the selected lines.
