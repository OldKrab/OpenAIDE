# Project Management Design

Status: accepted prototype design

This note records the approved Project-management experience for Web and Desktop. It defines the visible structure and behavior to preserve during production implementation. Internal ownership and protocol details may change while hardening, but visible or semantic changes require renewed product approval.

The approved reference is `packages/frontend/prototypes/project-navigation`, primarily the `footer-add`, `unavailable-project`, and `no-projects` variants.

## Product model

- A **Project** is the durable OpenAIDE context that owns a group of Tasks.
- A **Task Workspace** is where one Task's Agent operates. It is either the Project root or a worktree associated with that Project.
- Task Navigation remains titled **Tasks**. Projects organize Tasks; they do not replace Tasks as the primary work item.
- A Task's Project Context is fixed when the Task is created. OpenAIDE does not move existing Tasks to another Project folder.
- Renaming a Project changes only its OpenAIDE display name. Its original folder path and identity do not change.

## New Task start screen

The start screen keeps the centered question **What are we working on?** followed by one readable context sentence:

> Work on [Project] using [Agent] with [Isolation]

- Project, Agent, and available Isolation are inline selectors, not boxed form fields.
- The selected values remain visibly interactive through restrained shape, hover, focus, and open states. They do not rely on repeated chevrons that interrupt sentence readability.
- Selected values use ordinary readable weight rather than making the entire sentence bold.
- Icons use the shared neutral icon language. Accent color is reserved for focus and selection, not a persistent glow.
- The open selector is visibly selected. Long values truncate only in the sentence; the popup exposes the full value and supporting metadata.
- At narrow widths the connective words may disappear and the available selectors remain in Project, Agent, Isolation order without horizontal overflow.

### Project selector

- Project selection is the first and primary context choice.
- The popup lists Projects with display name and original path. The selected Project uses the standard selected-row treatment.
- Adding a Project is visually distinct from selecting an existing Project. It is an action in the popup toolbar, not another Project-shaped row.
- An unavailable Project remains selectable for inspection and is labeled **Folder unavailable** with its original path.

### Agent selector

- The Agent popup uses the same popup structure, row rhythm, selected state, and spacing as the Project and Task Workspace selectors.
- Agent choices remain visually identifiable as Agents through their own icons and labels; consistency does not erase role distinctions.

### Isolation and Task Workspace selector

- Isolation is the third choice in the sentence because it is an advanced decision and should not dominate the ordinary Project-first flow.
- The Isolation clause appears only when the selected Project supports worktrees. For a non-Git Project, running in the Project root is the only behavior and remains implicit: **Work on [Project] using [Agent]**.
- **No isolation** means the Task runs directly in the selected **Project root**.
- Existing worktrees are listed as Task Workspace choices and remain associated with the selected Project.
- **New worktree** and **Manage worktrees** are actions, not selectable workspace rows. They use a distinct toolbar/action treatment.
- New-worktree creation follows the accepted workflow in `docs/worktree-task-design.md`.

## Task Navigation sidebar

- The sidebar is Task-first and groups Tasks under Project headers.
- A Project header shows its folder icon, display name, Task count or unavailable state, collapse behavior, and a stable `…` action.
- Project rows and Task rows use the same density and hover-preview timing rules while remaining visually distinct through hierarchy and content.
- **Add project** is persistent in the sidebar footer immediately above **Settings**. It is not placed under Search or beside the **Tasks** heading.
- Empty Projects remain visible and explain that they have no Tasks yet.

### Project hover preview

- On pointer devices, the first dwell on either a Project or Task waits 750 ms before opening a passive preview.
- While any sidebar preview is open, moving directly between Project and Task rows updates the preview immediately, including cross-type movement.
- The row and preview share a short leave grace period. There is no artificial gap that resets the shared preview interaction.
- A Project preview shows the Project display name, Project or unavailable state, original folder path, Task count, and worktree count.
- Hover is passive. It does not open the Project, refresh the filesystem, acknowledge Task attention, or mutate product state.
- Mobile does not add a separate **Project details** command. The Project action menu includes the Project name and original path as context.

### Project actions and rename

- The Project action menu contains **New task in project**, **Rename project**, **Manage worktrees** when applicable, and **Remove project…**.
- Unavailable Projects replace **New task in project** with **Check folder again**.
- Rename is inline in the Project header. Enter or the explicit save control commits; Escape, the explicit cancel control, or clicking outside cancels.
- Rename never changes the folder path or Project identity.

## Adding a Project

Adding a Project chooses a folder and creates or selects the canonical Project record.

- Web opens the framed folder explorer. It starts in the home folder when that root is available, keeps the current directory as the selection, prefills the Project name from that folder, and supports Up, breadcrumbs, editable path, folder traversal, and **Add this folder**.
- The Project name follows the current folder until the user edits it.
- Desktop uses the operating system's native folder picker.
- VS Code uses the appropriate VS Code workspace or folder capability.
- Shell-specific folder selection stays behind the shell boundary; the shared Frontend receives a selected-folder result.
- If the canonical folder is already a Project, OpenAIDE selects that Project instead of creating a duplicate.
- Adding the first Project immediately selects it and returns to the approved New Task start screen.

## Removing a Project

- **Remove project…** is a destructive action and uses a clear destructive treatment.
- A Project with no Tasks receives a concise confirmation.
- A Project with task rows uses the same count shown in Task Navigation, including discovered Agent sessions presented there as Tasks.
- The confirmation states that those Tasks will be removed from OpenAIDE.
- The confirmation explains that the corresponding sessions remain available in their original Agent.
- The Project folder, files, and worktrees are never deleted or changed by Project removal.
- After removal, OpenAIDE selects the next adjacent Project, falling back to the previous Project when needed.
- If the open Task belongs to the removed Project, OpenAIDE navigates to New Task in that adjacent Project.
- Removing the last Project opens the approved no-Projects state.

## Unavailable Project folder

An unavailable folder may be a temporary filesystem, mount, permission, or device condition, so OpenAIDE keeps the Project record and its Tasks visible.

- The Project header, Project selector, and passive preview all show **Folder unavailable** and preserve the original path.
- Existing Task history remains readable.
- New Task composition for that Project is disabled while the folder is unavailable.
- Recovery offers **Check again** and **Remove project…**.
- **Check again** revalidates the same original path. It does not open a folder picker or change Project identity.
- OpenAIDE does not offer **Reconnect**, **Locate folder**, path migration, or Task migration in v1. If the folder has permanently moved, the user may remove the old Project and add the new folder as a new Project; Agent-owned sessions remain available in their original Agent.

## No-Projects state

- The main surface shows **No Projects**, explains that a Project folder is required to start a Task, and presents **Add project** as the primary action.
- The sidebar preserves the approved structure and footer **Add project** action.
- **New task**, Search, and Refresh are disabled because they have no Project context or Task collection to act on.
- If the state follows removal, a quiet confirmation states which Project and Tasks were removed from OpenAIDE.
- Adding a Project exits the empty state immediately and selects the new Project.

## Archive

- Archive uses the same Project grouping and Project-row rules as ordinary Task Navigation.
- Archive supports search and Task restore.
- Restoring a Task returns it to its Project group, selects that Project, and returns to open Task Navigation.
- Removing a Project also removes its archived OpenAIDE Tasks. Their Agent-owned sessions remain in the original Agent.

## Production hardening requirements

- App Server owns Project identity, canonical paths, availability, rename, removal, Task association, and Project collection ordering.
- Frontend owns popup visibility, hover timers, inline rename editing, modal visibility, and other ephemeral presentation state.
- App Shells own folder-picking capabilities and return candidate folder facts; they do not create or migrate Project identity themselves.
- Production tests must cover canonical duplicate add, rename without path mutation, unavailable-path refresh, Project removal with open and archived Tasks, adjacent selection after removal, last-Project removal, and shell-specific folder selection boundaries.
- The integrated UI must be compared against the approved prototype at realistic desktop and narrow sizes before the prototype is removed.
