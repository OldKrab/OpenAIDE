# Project file access: production integration

Status: accepted. Production integration of the reviewed project-access prototype.

## Product behavior

Web and Desktop expose Project files on the Task Page. Files, Search, and Changes share one navigator and reader. On desktop the navigator remains visible; narrow screens navigate between list and reader. Back to conversation restores the same Chat and unsent Composer draft. Existing Agent File References open this same reader. VS Code keeps native file opening.

File/Diff switching preserves tab chrome. Source and diff share syntax rendering and code metrics. Native text selection quotes exactly the selected text; gutter click, drag, and Shift-click select whole lines. Comment appears beside the completed selection and inserts an editable File Quote into Composer without sending. Removed diff lines retain their HEAD location. Touch input retains Select range; pointer input does not show it.

## Existing mechanism

Retain the existing App Server File Viewer registry, client-bound handles, bounded snapshots, explicit refresh/release, source and Markdown rendering, image inspection, download capabilities, and fallback states. Preserve explicit user opening of absolute paths outside the Task Workspace. Such files appear in the same reader but do not change the navigator's workspace.

Replace the current Web/Desktop split-panel presentation with the approved Project files destination. Keep one task-scoped tab owner. Task changes release file handles and clear browsing/selection state. Do not add a second viewer, persist tabs, or transfer the prototype's DOM-query/portal selection adapter into production. Selection belongs in the shared source/diff renderer through typed React state and callbacks.

## App Server boundary

App Server resolves the current Task Workspace from an authorized Task identity; the browser does not choose another root. Add read-only methods alongside File Viewer:

- `fileViewer/listDirectory`: Task identity, relative directory, and continuation; returns ordered entries and an explicit continuation/truncation state. Load directories on expansion.
- `fileViewer/search`: Task identity, filename/content mode, literal query, case matching, and continuation; returns relative paths and bounded line matches. The existing mention-oriented `task/searchFiles` remains compatible.
- `fileViewer/changes`: Task identity; returns branch/base context and changed paths, including added, deleted, renamed, untracked, and conflicted states. A non-Git workspace returns an explicit unavailable state.
- `fileViewer/diff`: Task identity and selected change; returns bounded authoritative Git hunks with old/new paths and line numbers, base identity, and explicit unsupported/truncated states.

Directory enumeration and search stay within the authorized Task Workspace, including canonical-path checks for traversal and directory symlinks. Explicit file opens retain the existing broader user-initiated authority. Git commands are read-only, do not run external diff drivers, and handle paths as data. A change result is not a durable file capability.

Filesystem scans and Git work run outside the shared protocol lock. Each request has bounded work/output and classified failure, cancellation or supersession handling, and metadata-only start/terminal diagnostics. Refresh is explicit initially. Results identify incomplete searches rather than presenting a capped result set as complete. Implementation must document concrete bounds and ignore behavior with the protocol types.

## Implementation and verification

1. Agree this boundary and update the accepted Task Page/File Viewer specifications, including ADRs 0033, 0035, 0038, and 0039 and the File Quote definition.
2. Implement the App Server operations, generated TypeScript bindings, authorization, and boundary tests. Cover workspace/worktree resolution, denied/traversing paths, pagination/limits, non-Git workspaces, and Git additions/deletions/renames/conflicts.
3. Integrate the navigator with the existing tab/read lifecycle through the central Frontend intent layer. Discard stale responses on Task, workspace, query, and connection changes; release replaced handles.
4. Implement shared source/diff selection and contextual actions without DOM adaptation. Preserve preview, download, error, refresh, and quote behavior through the existing viewer.
5. Verify real repository workflows in Web and Desktop composition, plus the unchanged VS Code native-file path. Exercise actual pointer dragging, text selection, keyboard access, narrow layouts, themes, draft preservation, and async races.

Runtime deployment remains a separate user-requested action.

## Implementation verification

The integration reuses the existing file capability registry, image/download surface, and Task-owned tabs. The former split-panel presentation is replaced by Project files; VS Code retains native file opening.

Verified through the real App Server boundary and browser UI:

- Task initialization/authorization, workspace containment, symlink exclusions, pagination, ignore behavior, and explicit search limits.
- Git additions, deletions, renames, untracked and binary files, unborn repositories, linked worktrees, conflicts, and literal wildcard filenames.
- Independent tree/filename/content browsing, exact search-line focus, File/Diff continuity, gutter drag, native text selection, contextual comments, and draft preservation.
- Existing Markdown preview, image zoom/preview, original-byte downloads, missing-file retry, and returning to the mounted conversation/Plan.
- Web wide/mobile layouts and Desktop macOS/Windows composition at wide and supported minimum window sizes. Explicit Chat references restore source after the same file was viewed as a diff.
- Reopening Project files retains the explorer scroll position while cached directories refresh.
- Late file-open completion after Task switching is discarded and its capability released. Existing VS Code native-file messaging tests pass.

Validation commands: the `project_files` Rust test filter, the frontend File Viewer/lifecycle/Markdown/highlighting tests, VS Code webview messaging tests, `file-viewer-layout.spec.mjs` in Web and Desktop composition, frontend TypeScript checking, generated-protocol consistency, source-size policy, and `git diff --check`.

Native macOS/Windows installer builds and OS-level testing were not performed. Browser smoke tests use disposable local server state; runtime deployment remains separate.
