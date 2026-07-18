# Prepared Task Pool and Worktree Tasks

Status: accepted implementation design

Related ticket: [#17, Create Tasks in dedicated Git worktrees](https://github.com/OldKrab/OpenAIDE/issues/17)

This note records the accepted worktree design. Parts 1 and 2 are implemented and must remain consistent with the accepted Task Lifecycle and Chat specification.

## Delivery split

### 1. Leased prepared-Task pool

Replace the current one-New-Task-per-client model with reusable zero-turn prepared Tasks.

Implemented and merged before worktree behavior depends on it.

### 2. Worktree Task Workspaces

Implement worktree discovery, creation, selection, Task binding, and management. Worktree Tasks reuse the prepared-Task leasing mechanism rather than introduce another provisional-session lifecycle.

## Shared vocabulary

- **Task Workspace**: the filesystem work area in which a Task's Agent operates. It can be the Project checkout or a dedicated Git worktree while the Task remains associated with the originating Project Context.
- **Prepared Task**: a durable zero-turn New Task with an Agent Native Session. It has not accepted its first User message and is not visible in normal Task Navigation.
- **Prepared-Task lease**: exclusive assignment of one Prepared Task to one live client while that client's New Task composer uses it.
- **Free Prepared Task**: an unleased zero-turn Prepared Task eligible for reuse.
- **Worktree management**: repository-scoped discovery and maintenance of Git worktrees. It is separate from Task creation and Task Archive.

## Part 1: leased prepared-Task pool

### Accepted direction

- App Server owns a pool keyed by `(Agent, canonical Task Workspace folder)`.
- The pool contains Prepared Tasks, not bare Native Sessions. Each Prepared Task remains the sole owner of its Native Session.
- Free and leased ownership is authoritative durable Task lifecycle state. `TaskLifecycle::New` contains an optional client lease; the App Server derives its pool index from Task records rather than persisting a second ownership map.
- Preparation readiness remains separate from lifecycle ownership. Only a ready, zero-message, unleased New Task is eligible for reuse.
- A client requesting a New Task atomically leases a matching free Prepared Task when one exists.
- A leased Prepared Task is exclusive to that client. Native Sessions are never simultaneously shared by multiple Tasks or clients.
- One client holds at most one Prepared-Task lease.
- Prepared-Task leasing uses simple server-ordered lock semantics. A client asks to acquire a pool key and later releases its current lease; it waits for release acknowledgement before acquiring a different key.
- App Server serializes acquire, release, first Send, confirmed client expiry, and restart recovery. Authoritative lease release commits synchronously before any later acquisition, while Native Session cleanup may continue afterward.
- Client and Task identity are sufficient under this ordering model. Prepared-Task leases do not add a generation or lease token.
- If no free match exists, App Server creates and prepares another zero-turn Task for that key.
- First Send permanently removes the Prepared Task from the pool and promotes that same Task and Native Session into the visible Task lifecycle.
- Confirmed client expiry releases its zero-turn lease. A transient transport disconnect does not release it; the existing reconnect grace and stable `clientInstanceId` semantics apply.
- App Server restart immediately clears all persisted zero-turn leases. A reconnecting client may lease the same free entry, but no client retains a reservation across server restart.
- After restart, App Server rebuilds the free index from durable New Tasks but restores a Native Session only when its key is leased. It attempts load or resume when possible and recreates a missing session for the same Prepared Task when necessary.
- On upgrade from the one-New-Task-per-client model, App Server clears legacy owners and adopts eligible durable zero-message New Tasks as free Prepared-Task candidates. It keeps the newest eligible entry per pool key, applies the global free-entry cap, and closes extras; it never restores legacy ownership as a lease across restart.
- Changing Project, Agent, or Task Workspace releases the client's previous Prepared-Task lease immediately. Switching back reuses that session only if it remains the free entry and another client or LRU eviction has not taken it.
- Ordinary navigation to Settings or an existing Task retains the client's current Prepared-Task lease, matching the current New Task behavior. There is no new user-facing action for keeping or destroying a zero-turn session.
- Lease release and disposal are driven by context change, confirmed client expiry, App Server restart, pool eviction, and Task Workspace removal.
- Disabling or deleting an Agent disposes every zero-message Prepared Task for that Agent, including leased entries, and closes their Native Sessions. Affected Frontends retain the composer unchanged while selecting or preparing another Agent.
- The public pre-history lifecycle exposes release, not discard. Frontend releases a Prepared-Task lease; only App Server pool policy destroys excess, failed, evicted, or workspace-invalid Prepared Tasks. The existing `task/discard` operation is replaced rather than retained with changed semantics.
- Browser-tab `clientInstanceId` already survives reload. A duplicated or newly opened tab remains a distinct client.
- At most one free Prepared Task is retained for each `(Agent, canonical folder)` key. Extra sessions created for concurrent leases are closed when released if a free entry already exists.
- Free Prepared Tasks across different keys are bounded by a global least-recently-used cap. Eviction applies only to unleased zero-turn Tasks.
- Free-entry recency is the durable time at which a retained Prepared Task most recently entered the free pool. Acquiring removes it from eviction consideration; releasing and retaining it makes it newest. When the cap is exceeded, App Server evicts the oldest free entry, with Task id as the deterministic tie-breaker.
- The exact initial global cap is an implementation constant to tune from measurements, not a user-facing setting in the first version.
- Frontend receives only its currently acquired Prepared Task through the normal Task snapshot and readiness event stream. Free entries, pool counts, pool keys, LRU order, eviction metadata, and disposal decisions remain App Server-internal state.
- Normal diagnostics record acquire, reuse, create, release, retain, dispose, and evict outcomes with Task id and reason, plus aggregate pool counts. They omit canonical workspace paths, client ids, prompts, Agent configuration values, and stable hashed pool keys.
- Prompt text remains Frontend-owned. A free Prepared Task does not retain a composer draft.
- The entire unsent composer has live-Frontend lifetime only. Prompt text and Images survive context changes, ordinary navigation, reconnect, and App Server restart while that Frontend remains loaded; a full Frontend reload or tab closure discards them. Part 1 adds no browser-persisted or App Server-owned Draft.
- Releasing a lease preserves the Native Session's current Agent configuration values. A later lessee sees and may change the exact values reported by that reused session; OpenAIDE does not attempt a generic reset to Agent defaults.
- Multiple visible Tasks may use the same Task Workspace. Each visible Task still has its own Native Session and conversation history.

### Composer resources

- Typing `@` at the beginning of the prompt or after whitespace opens workspace-file autocomplete, following the same assistance-only model as slash-command completion. Selecting a result inserts ordinary prompt text; it does not create a structured mention, attachment handle, origin binding, or separate draft state.
- Completion inserts `@relative/path` for ordinary paths and `@"relative/path with spaces"` when whitespace requires an explicit endpoint. Both forms remain ordinary prompt text.
- File completion uses OpenAIDE's workspace file catalog rather than treating every `@word` as a file. Email addresses and unmatched `@` text remain ordinary prompt text.
- The file-completion catalog is scoped to the current Task Workspace. It includes tracked files and non-ignored untracked files, excludes Git-ignored files and directories, and does not merge entries from the originating checkout or other allowed roots.
- Git exclusions follow the repository's effective rules: nested `.gitignore` files, `.git/info/exclude`, configured global excludes, and unconditional omission of Git administrative storage. A non-Git workspace has no Git exclusions. Changes to effective ignore rules invalidate and rebuild the affected index.
- Eligible symbolic-link paths may appear in completion results, but indexing never traverses symbolic-link directories.
- Completion is query-based rather than a full catalog snapshot. Frontend sends the fragment being completed; App Server returns a bounded, ranked result set for the current Task Workspace.
- Changing context preserves the text unchanged and gives it no hidden binding to the context in which autocomplete originally inserted it. The new context's catalog determines whether that text still identifies a file.
- The composer menu's workspace-file browsing action is removed. Image upload remains available as a separate input path because device-local image bytes cannot be represented by an Agent-accessible file link.
- Paste, drag/drop, and image picker are input methods for the same **Image** composer content; they are not separate attachment kinds.
- Prompt text, including inserted file-reference text, and images remain with the Frontend-owned composer draft when Project, Agent, Task Workspace, or Prepared-Task lease changes.
- Before Send, Frontend owns Image bytes, safe metadata, ordering, and previews. Adding an Image does not upload it to App Server or create a Task-scoped handle.
- Image bytes cross the App Server boundary inline in `task/send` only after the user invokes Send. Prompt text and all Images are validated and durably accepted together under a message-level aggregate size limit; no Send-time upload handles or binary transport are introduced for this slice.
- Failed Send validation retains the Frontend-owned Image; durable message acceptance transfers ownership into Task Chat before ACP delivery begins.
- When the selected Agent does not advertise ACP Image prompt capability, Frontend retains Images unchanged and disables Send with an explicit capability blocker. Removing the Images or selecting a capable Agent restores eligibility; OpenAIDE never silently drops Image content.
- The first `@file` slice sends the prompt as text only. Autocomplete and special rendering do not add ACP `resource_link` or embedded `resource` blocks and do not change the text delivered to the Agent.
- Recognized `@file` text receives special rendering in the composer and persisted User messages, without click or open-file behavior in the first slice.
- Persisted User messages decorate file-reference syntax without consulting the live workspace index. File deletion, rename, or later ignore-rule changes therefore do not alter historical rendering, and no structured match spans are stored.
- Structured ACP resource delivery and arbitrary device-file upload require a separate explicit design.
- Arbitrary device-file upload is a follow-up, not part of the prepared-Task pool slice. It should reuse the same client-owned pre-Send composer model, while its ACP representation, capability requirements, limits, persistence, and validation are designed separately.

### App Server Protocol boundary

- `task/acquire` replaces `task/create` for the New Task surface. It carries the selected Project, Agent, and Task Workspace identity; App Server resolves the canonical pool key and returns the leased Task's ordinary `TaskSnapshot` immediately, including `preparing`, `ready`, or recoverable preparation failure state.
- Acquiring the key already leased by that client is idempotent and returns the same Prepared Task. Frontend releases and awaits acknowledgement before acquiring a different key.
- `task/release` replaces `task/discard`. It releases that client's current zero-message lease and returns only an acknowledgement; releasing when no lease remains is an idempotent no-op.
- `task/send` continues to identify the exact Task and now carries prompt text plus inline Images. App Server accepts content only when that Task is ready and currently leased by the requesting client; it never redirects a Send into another Prepared Task.
- Existing Task preparation, Agent configuration, command, capability, and Send-readiness snapshot fields remain the single Frontend projection. No lease token, pool inventory, free count, LRU metadata, or pool-specific readiness representation is added.
- `client/initialize` gains no Prepared-Task or pool field. Frontend acquires when its live New Task composer needs a Prepared Task and otherwise leaves any retained lease alone. The existing `requestedSurface` bootstrap field is not an ownership or recovery input for the pool.

### Authoritative ordering outcomes

- First Send and release serialize on the Task lifecycle. Send-first atomically makes the Task visible, after which release cannot alter it; release-first removes authorization, so a later Send is rejected with the Frontend composer unchanged.
- Confirmed client expiry synchronously releases its lease before a reconnect can acquire. A transient disconnect does not enter this path.
- App Server startup clears every lease before accepting client requests. A live Frontend reacquires its selected key when it next needs the New Task composer; no accepted User message is replayed.
- Preparation failure remains attached to the leased Prepared Task for explicit retry or context change. Releasing a failed Prepared Task disposes it because failed entries are not eligible for the free pool.

#### Workspace file-index module

The App Server provides file completion through a dedicated workspace file-index module. The module owns workspace-wide discovery, Git-ignore filtering, filesystem watching, incremental index maintenance, ranking, bounded results, and path-safety rules behind a small query interface. It is separate from the existing attachment file browser, whose Task-owned expiring entry handles and directory browsing solve a different problem.

App Server shares one index per canonical Task Workspace folder across clients, Prepared Tasks, and visible Tasks; a Prepared-Task lease selects the workspace to query but does not own the index or watcher. Indexes and watchers are created lazily on first search, retained while recently used, and evicted only while idle under a global LRU cap. Initial idle duration, cache cap, and search-result cap are implementation constants to tune from measurements.

Watcher overflow, event loss, and effective-ignore-rule changes mark an index stale and trigger a full rebuild. Searches may return last-known matches while that rebuild runs. Refresh remains quiet: it neither clears visible results nor adds a persistent refreshing footer. Completion becomes unavailable only when the rebuild fails; failures are logged and surfaced in the picker rather than silently leaving the index permanently stale.

The module's external interface exposes search by canonical workspace and explicit forgetting of a removed workspace. Watcher start/stop, event application, rebuild, ranking, cache generations, and eviction remain internal. Task and lease authorization resolve the canonical workspace before crossing this seam; explicit worktree removal calls the forgetting operation.

- Search results expose only ordered, workspace-relative path strings plus overall index state. Ranking scores, match ranges, absolute paths, entry handles, watcher generations, and cache metadata remain internal; Frontend derives the basename, parent label, and file icon from the relative path.
- Paths that cannot be represented as valid UTF-8 are excluded rather than lossily inserted into prompt text. Diagnostics may report aggregate exclusion counts but must not emit raw path bytes.

App Server Protocol file-search requests identify the current leased Prepared Task or visible Task by Task id. App Server authorizes that Task for the requesting client and derives its canonical Task Workspace before querying the shared index. Frontend never supplies a workspace path. Frontend also associates each request with its current composer-context generation and ignores a late response after Project, Agent, Task Workspace, or lease replacement.

- File completion becomes available as soon as a Prepared Task lease establishes an authoritative Task id and Task Workspace. It does not wait for Native Session readiness or Agent prompt capabilities. A context change closes the open picker, preserves prompt text, and invalidates outstanding search responses from the previous composer-context generation.

The first search for an uncached workspace keeps the picker open in an `Indexing files…` state until one complete index is ready; partial results are not streamed. Later query changes keep the prior ranked results visible while the next in-memory ranking request settles. Internally, the module begins observing changes before the initial scan, discards watcher setup noise already covered by that scan, ignores `.git` metadata activity, and falls back to a full rebuild if event continuity is lost. The picker derives compact IDE-style icons from the returned path's filename and extension, with a generic fallback.

Watcher events update the shared index without publishing picker subscriptions. An already-open picker receives changed results only when its search query next changes; the first version neither pushes nor polls file-search results.

File ranking uses path relevance only in the first version. Exact matches rank before prefixes and prefixes before fuzzy matches; basename relevance wins over equivalent directory-only relevance; ties use stable lexical ordering. The index does not own or persist recent-file or recent-selection history.

The implementation should use maintained libraries rather than custom traversal, watching, or fuzzy-matching algorithms. Current candidates are `ignore` for walking with effective Git exclusions, `notify::RecommendedWatcher` for platform filesystem events, and `nucleo` for path-aware fuzzy matching. `ignore` must be configured to honor Git ignore sources but not ripgrep-style `.ignore` files, to include ordinary hidden files, and never to follow symbolic-link directories. Library types, scores, generations, and event shapes remain internal to the workspace file-index module.

An empty query immediately returns a bounded shallow-first list: root files before deeper paths, then stable lexical order. Once the user types a query, path-relevance ranking replaces shallow-first ordering.

### Current-code change

The current invariant is one client-private New Task per `clientInstanceId`. Changing Project, Agent, or workspace discards that Task and closes its Native Session. The pool replaces that uniqueness rule; the existing `(Project, workspace, Agent)` frontend preparation key is not already a server-side cache.

### Part 1 status

The leased prepared-Task pool is implemented. The remaining decisions in this note concern Part 2.

## Part 2: Worktree Task Workspaces

### Repository identity and scope

- A Worktree Repository is identified locally by its canonical Git common-directory path, obtained through Git rather than inferred from a remote URL or branch checkout path.
- The identity is shared by the primary checkout and all linked worktrees. It does not attempt to unify separate local clones of the same remote.
- Moving or repairing a repository may change or restore this local identity; cross-location identity recovery is not part of the first worktree slice.
- Managed worktrees live below the App Server state root at `worktrees/<repository-id>/<worktree-id>`. Both path components are collision-resistant opaque identifiers; prompt text, repository labels, and branch names do not become storage path components.
- A Managed Worktree has durable OpenAIDE metadata keyed by its opaque worktree identity. Its user-facing repository label, branch, ownership, and current Git state are projections rather than filesystem identity.
- Every Task backed by a worktree stores a `WorktreeId` reference. Tasks sharing a worktree reference one durable Worktree record, which owns repository identity, canonical worktree-root path, Managed/External class, availability, and last observed Git projection.
- Worktree support requires the Project folder itself to be Git's reported top-level working tree. The check accepts both a primary checkout's `.git` directory and a linked worktree's `.git` file by comparing the canonical Project root with `git rev-parse --show-toplevel`; it does not infer support by walking up from a nested Project folder.
- A worktree-backed Task retains its Project Context and resolves the Agent cwd directly to the selected worktree root. It does not copy the Worktree record's mutable fields.
- Every successful Git discovery synchronizes a durable Worktree record for each listed primary or linked worktree, even before a Task uses it. Management and Task Workspace selectors therefore operate on stable `WorktreeId` values rather than raw paths or transient candidate handles.
- A failed discovery does not mutate existing Worktree records or infer that previously known worktrees disappeared.
- After a successful refresh, a previously known Worktree record absent from Git is deleted immediately when no visible, idle, archived, or Prepared Task references it. Referenced records remain durable and become unavailable for Task history and recreation.
- App Server owns one command queue per Worktree Repository. Create, recreate, remove, and explicit refresh operations serialize through that queue; operations for different repositories may run concurrently.
- Each mutation performs its final Git discovery and durable Worktree-record synchronization before the next command begins. Git's own locks remain a safety layer, not OpenAIDE's product-ordering mechanism.
- V1 does not poll worktrees or keep filesystem watchers for external worktree changes. Discovery refreshes when the Task Workspace selector or management surface opens, before a worktree is used, after a worktree mutation, and on explicit Refresh.
- When a worktree-backed turn becomes terminal, App Server cheaply checks whether its workspace root still exists. A missing root triggers repository discovery and the ordinary unavailable-workspace behavior; OpenAIDE does not try to interpret unstructured Agent error text as filesystem state.

### Task creation UX

- **Task Workspace** is the authoritative filesystem choice in the existing Task start context row with Project and Agent. The existing user-selected `IsolationKind` and composer isolation menu are removed rather than moved.
- Intended control order is Project, Task Workspace, Agent.
- Task Workspace choices are the current checkout, an existing repository worktree, or a new worktree.
- A fresh New Task defaults to **Project root**. OpenAIDE does not remember the last selected worktree or open New worktree automatically; isolation remains an explicit choice for each Task.
- **Project root** is visually distinct from linked worktrees through its folder icon and name. It means the selected Project's configured root, not the browser's current directory or an ambient process directory.
- For a Project outside Git, or one nested below a repository root without being its own Git top level, the control remains visible with **Project root** as its sole value. New worktree and Manage worktrees are hidden, and ordinary local Tasks remain available.
- A Git discovery failure for an otherwise usable Project does not block Project root. The worktree choices become unavailable with an inline error and explicit refresh.
- Local/worktree presentation is derived from the selected Task Workspace and is not separately stored or accepted as user input. Legacy `local` Tasks migrate to their existing stored workspace path.
- `docker` is removed from this model. A future container execution environment requires its own design and does not masquerade as a filesystem Task Workspace.
- The composer retains prompt text while Task Workspace preparation changes. The final worktree Native Session exists before the user starts the Task, so its Agent options and slash commands are authoritative before Send.
- Creating a worktree is explicit preparation before Send. The Agent is never started against an empty placeholder folder.
- Selecting **New worktree** opens a compact form anchored to the Task Workspace control, not a modal and not composer content. It shows Base = current committed `HEAD`, Create branch = off, and an explicit **Create** action; accepting the defaults takes one confirmation click.
- During creation, Project and Task Workspace controls are locked to the operation's repository context. Agent selection, prompt and Image editing, and ordinary navigation remain available.
- The currently selected workspace's Prepared Task remains leased while filesystem preparation runs, so its Agent options and slash commands stay available. Only successful worktree preparation changes Task Workspace: App Server then releases the old lease and acquires the new `(Agent, worktree)` key. Failure leaves the existing lease and selection unchanged; one client never keeps both leases.
- Worktree creation is App Server-owned and continues when the user navigates away. Returning to New Task renders its latest progress; completion applies the new Task Workspace to that retained context and uses the latest selected Agent when acquiring its Prepared Task.
- A full Frontend reload or disconnect does not reattach that client to its in-flight creation operation, even when the stable client id reconnects. The App Server lets the operation finish, but it no longer auto-selects the result for that client's New Task context; the resulting worktree is available through ordinary discovery and manual selection.
- While such an operation remains active, its target is ephemeral repository operation state and cannot be selected or removed. This prevents a reloaded or second client from using a Git-registered worktree before checkout and `.worktreeinclude` preparation finish; terminal success or failure returns it to the ordinary discovery rules without persisting a separate failed-worktree lifecycle.
- An App Server restart drops in-memory worktree progress, queued operations, and live-client result application. Startup performs ordinary Git discovery; any listed worktree is projected normally, with no resumed copy, automatic selection, rollback, or persisted incomplete state.
- After Create, the compact form closes. The Task Workspace control shows a busy state, while the existing quiet New Task status line reports stages and measurable copy progress such as checkout creation, local-file copy counts/bytes, and Agent preparation.
- Worktree Tasks remain grouped under the originating Project Context.

### Approved Task Workspace UI

- The production implementation follows `packages/frontend/prototypes/worktree-ux-directions`, variant A, for the Task Workspace chooser, anchored creation form, progress, failure, Task header context, and worktree management surface.
- The chooser is titled **Task workspace** and explains that it selects where the Task runs. It is a compact popover anchored to the Project, Task Workspace, Agent context row rather than composer content or a modal.
- The selected row is communicated by its quiet selected background; it does not add a leading checkmark or extra indentation. The Project root and worktrees share one list rather than being split into separate visual panels.
- A long repository list scrolls within the chooser, keeps search and footer actions stable, and uses a subtle bottom fade to indicate more results. Footer actions are **New worktree** and **Manage worktrees**.
- The creation view replaces the chooser body in the same anchored surface. A back action returns to selection. Its fields use the same typography and controls as the management-page creation view.
- On mobile, the anchored surface becomes a closeable, viewport-contained sheet without adding a second line to compact Task rows or headers.
- Prompt text and Images remain unchanged while the user opens, closes, creates, or changes Task Workspace.

### Worktree creation

- Default base revision is the source checkout's committed `HEAD`.
- The v1 base picker offers that committed `HEAD` plus local branches only. It does not fetch, list remote-tracking branches or tags, or accept arbitrary commit expressions.
- Uncommitted and untracked source-checkout files are not applied to the new worktree. The creation view explains that worktrees start from committed files only; it does not pretend to show a live clean/dirty state.
- New Managed Worktrees default to detached `HEAD` at the selected base commit. Creating a branch is optional and off by default.
- With Create branch off, the form relies on its visible detached-`HEAD` value and adds no separate warning copy. The removal preflight remains responsible for blocking deletion when detached commits have not been preserved.
- Worktree creation leaves Git submodules uninitialized. OpenAIDE does not automatically run `git submodule update --init --recursive`; the user or Agent may initialize submodules explicitly after creation when the Task needs them.
- Enabling **Create branch** reveals a branch-name field pre-filled with a readable plain slug derived from the worktree name. It adds no mandatory OpenAIDE prefix, and the entire branch name remains editable before creation.
- The suggestion continues to follow worktree-name edits until the user manually edits the branch field. After that first manual branch edit, later worktree-name changes never rewrite it.
- Suggestion generation preserves Unicode letters and numbers from every script, converts whitespace and punctuation runs to single hyphens, and trims separators. It does not transliterate to ASCII.
- The generated slug is limited to 48 user-perceived characters before any collision suffix. A manually edited branch may be longer when Git accepts it.
- The initial suggestion receives a short uniqueness suffix only when that unsuffixed suggestion already collides with an existing local branch. The visible field always shows the exact branch that will be created.
- The final edited value is validated with Git branch-name rules.
- The base picker lists existing local branches. When Create branch is enabled, a branch-name collision disables creation; OpenAIDE revalidates during `git worktree add` to handle races and never silently adopts the colliding branch.
- A normalization-empty worktree name blocks creation. If Create branch is enabled and no usable branch slug exists, the branch field remains empty and creation is blocked until the user enters a valid name; OpenAIDE does not generate an opaque fallback.
- Managed Worktrees live under the App Server state root, organized by opaque repository and worktree identity.
- OpenAIDE behaviorally follows `satococoa/git-worktreeinclude`, implemented as a focused Rust module rather than a dependency on a generic recursive-copy crate or the Go binary.
- The current Project checkout supplies `.worktreeinclude` and the ignored source files, even when it is itself a linked worktree. OpenAIDE does not default to the repository's primary checkout or expose another source picker.
- Eligibility is the intersection of NUL-delimited paths Git reports as ignored by effective repository rules and paths Git reports through the Git-ignore-compatible `.worktreeinclude` patterns. Tracked files never enter the copy plan, and a missing source `.worktreeinclude` is a successful no-op.
- Paths must remain within the source and destination roots. Only regular files are copied; symbolic links and other non-regular entries are skipped rather than followed or recreated.
- Each file is copied through a temporary sibling and atomic rename, preserving ordinary permission bits. Existing equal files are skipped; differing destinations are conflicts rather than overwritten.
- Copy atomicity is per file, not for the full operation. Processing collects per-path outcomes, and any copy errors fail worktree preparation after the plan has run; already copied files remain because the worktree workflow has no rollback subsystem.
- `.worktreeinclude` is an explicit repository instruction to copy ignored local content, including files that may contain secrets. OpenAIDE does not infer, inspect, or log file contents.
- The copy plan has no OpenAIDE byte limit, file-count limit, or large-copy confirmation. Files are streamed rather than buffered as a complete payload.
- The copy stage reports measurable file and byte progress but is not user-cancellable in v1.
- A partial copy error reports filesystem preparation failure and refreshes Git discovery. If Git lists the worktree, it immediately becomes an ordinary selectable worktree with its location-derived Managed/External class, no persisted incomplete state, and no special retry action; copied files remain in place.
- A failure after Git registration does not select the resulting worktree for New Task. The previous Task Workspace remains selected; the error identifies the failed stage, and the discovered Git-valid worktree is available for later explicit selection or management.
- Any failed creation or later filesystem-preparation step produces an error notification identifying the failed step.
- There is no automatic rollback subsystem and no special persisted failed-worktree state.
- After an error, OpenAIDE refreshes Git discovery. Any worktree Git recognizes appears normally in the worktree list and can be inspected, selected, or removed through ordinary management.
- Once checkout and `.worktreeinclude` preparation succeed, the new Task Workspace remains selected even if its independently acquired Prepared Task later fails to initialize the Agent. That failure uses the ordinary recoverable Task-preparation state and does not revert or remove the valid worktree.
- Recreating an unavailable Task Workspace reuses this same creation flow. The recorded destination path is fixed. A recorded branch enables the branch field and pre-fills it; a formerly detached workspace leaves branch creation off. Base selection, validation, Git creation, `.worktreeinclude`, progress, and errors otherwise use the ordinary rules.
- In recreation, an available prefilled existing local branch is checked out explicitly. The user may instead disable the branch field for detached creation or replace it with a new branch name, in which case the ordinary base picker applies. OpenAIDE stores no separate recovery-base or failed-recreation model.
- Recreate workspace applies equally to Managed and External Worktrees with no extra confirmation. Management class is determined by whether the recorded path is under OpenAIDE-managed storage, not by which tool performs recreation.
- Recreate adds no special handling for a stale Git worktree registration whose folder is missing. OpenAIDE does not force, prune, or repair it; the ordinary Git error is surfaced and repository repair remains external.

### Existing worktrees and Tasks

- Discover all worktrees returned by Git's stable NUL-delimited porcelain listing, including worktrees created by other tools and registrations whose folders are unavailable.
- Projects whose roots are separate top-level worktrees of the same repository share one repository-scoped inventory and management surface.
- Label worktrees as **Managed Worktree** or **External Worktree** by merging Git discovery with OpenAIDE metadata.
- Any valid worktree may be selected as a Task Workspace.
- A locked worktree remains selectable when its folder is available, but its locked state and optional Git-provided reason are visible and removal is disabled.
- A prunable or otherwise missing registration remains visible as an **Unavailable Worktree** with Git's reason. Use is disabled; management offers Refresh, Recreate, and **Forget worktree**. Forget removes the stale entry from active worktree inventory without touching Git or Task history.
- OpenAIDE does not add a special failed-worktree lifecycle or persisted recovery state for unavailable registrations.
- If successful discovery marks a running Task's worktree unavailable because of an external change, OpenAIDE immediately shows the unavailable state and rejects new Sends but does not cancel the active turn. That turn may complete, fail naturally, or be cancelled by the user.
- After that turn becomes terminal, OpenAIDE closes its Native Session while the workspace remains unavailable. Task history is preserved; the session is loaded or recreated only after explicit Recreate restores the recorded path.
- Multiple Tasks may use the same worktree concurrently. Reusing a worktree does not mean those Tasks are isolated from each other, and the UI must expose linked and running Task counts.
- Selecting a worktree already used by a running Task is allowed immediately. OpenAIDE relies on the visible running-Task count and does not add a confirmation or serialize Agent access to the shared files.
- Selecting a worktree that already has Tasks creates or leases a separate zero-turn Prepared Task and Native Session. It never clears or reuses a visible Task's conversation.

### Worktree management UX

- Worktree deletion is not part of creating a New Task.
- The primary entry is **Manage worktrees** in the Project actions menu in Task Navigation.
- A secondary **Manage worktrees** shortcut appears in the New Task Workspace selector.
- An unavailable Project that retains a durable Worktree Repository association keeps its Project action entry to management so the recorded root can be inspected or recreated.
- Management opens a repository-scoped central surface, not a modal and not a permanent sidebar section.
- The panel lists the primary checkout and every linked worktree with path, branch or detached `HEAD`, availability or lock state, linked Task count, running or leased activity, last use, and Managed/External ownership. It does not continuously compute clean/dirty filesystem status.
- The primary checkout is visible context but cannot be removed.
- A linked worktree that contains a configured Project root may be removed after the ordinary safeguards. OpenAIDE retains that Project and its Task groups but marks the Project unavailable; it cannot start New Tasks in Project root or another worktree until its recorded Project folder is recreated. Existing Tasks whose own Task Workspaces remain available may continue.
- Supporting that outcome requires explicit Project availability in the App Server-owned Project projection; the current id-and-label-only Project summary is insufficient. The unavailable Project remains visible for history and recovery, and a later refresh restores availability when the recorded Project root exists again.
- Worktree actions are: **Use for New Task**, **Open folder** when supported, **View linked Tasks**, **Refresh**, **Remove worktree**, and **Forget worktree** for an unavailable entry.
- **Use for New Task** returns to the live New Task surface, preserves its text and Images unchanged, selects the chosen Task Workspace, and performs the ordinary Prepared-Task release/acquire transition. When management was opened outside New Task, the action opens New Task with that context and retains any still-live Frontend-owned composer.
- **Open folder** is capability-gated App Shell behavior distinct from opening a file. VS Code focuses the folder in its Explorer when it belongs to the current VS Code workspace; otherwise it opens the directory in the OS file manager. Desktop opens it in the OS file manager. Web hides the action because it cannot reliably open arbitrary local directories.
- Folder opening uses a dedicated shell capability and directory-path authorization. It does not reuse the current text-document opening path, whose authorization is intentionally limited to files inside the active VS Code workspace.
- OpenAIDE may remove both Managed and External Worktrees after the same safety preflight. External removal explicitly identifies the path and that another tool created it.
- Worktree management may create a standalone Managed Worktree through the same creation view used by New Task. It does not automatically select that worktree for a Task when management was opened independently.
- Worktree management does not rename or delete branches, merge, rebase, create pull requests, or automatically clean up after Task Archive.
- Task **Archive** already provides the product's close/history lifecycle. Do not add a second Close state solely for worktrees.

### Approved Worktree Management UI

- The production implementation follows the approved management scene in `packages/frontend/prototypes/worktree-ux-directions`, variant A.
- Management is a central repository-scoped page. Desktop uses a compact worktree list beside one detail surface; it is neither a full-width sparse table nor a stack of bordered cards. Narrow screens show the same list and detail content as sequential views with an explicit back/close affordance.
- The list header keeps **New worktree** and Refresh together on the left. Refresh is a quiet icon action with an accessible label and tooltip, not a distant floating control or a one-item overflow menu.
- Worktree rows stay compact and use stable identity, state, activity, and action slots. The selected worktree uses one quiet highlight. Paths and detailed metadata belong in the detail surface rather than every row.
- The detail surface shows the real filesystem path, visually truncated when necessary, with capability-gated Copy and Open folder actions. It never substitutes product copy such as “OpenAIDE storage” for the path.
- **New task here** is the primary worktree action and sits with the worktree identity before lower-priority metadata. Linked Tasks are a visually distinct, internally scrollable list that reuses the compact Task-navigation row language and supports filtering for large histories.
- Worktree display-name editing changes only OpenAIDE metadata. Git branch and folder names remain unchanged.

### App Server Protocol boundary

- Worktree inventory and repository-operation state use one authoritative repository-scoped state subscription. Projects whose roots are separate worktrees of the same Worktree Repository consume the same subscription rather than owning duplicated worktree snapshots.
- The subscription snapshot contains the durable Worktree records visible for that repository plus active and queued create, recreate, remove, or refresh operations. Mutation responses acknowledge or return their direct result, while the ordered subscription remains authoritative for subsequent shared state.
- Detailed progress remains available to the connected client that initiated creation through the shared operation projection and its New Task status line. Other subscribed clients may show the same repository operation as preparing or busy; the operation is not stored in a Task snapshot or composer draft.
- Frontend protocol values identify repositories and worktrees with opaque App Server ids. Frontend never supplies a filesystem destination for Managed Worktree creation or a raw path in place of a `WorktreeId`.
- Project projection exposes an optional opaque Worktree Repository id only when the Project root itself is a supported Git top level. Frontend uses that id to subscribe and invoke repository operations; App Server remains responsible for resolving and authorizing every filesystem path.
- Part 2 replaces `task/acquire.workspaceRoot` with a tagged Task Workspace identity: Project root is derived from the selected Project, while a linked worktree is identified by `WorktreeId`. App Server validates that both belong to the same Worktree Repository before resolving the canonical pool folder.
- Worktree mutations are `worktree/create`, `worktree/recreate`, `worktree/remove`, and `worktree/refresh`. Long operations return an accepted operation id promptly; their repository subscription projection is the single source for queued, running, progress, success, and failure state.
- `worktree/create` carries the selected base identity and the exact optional branch name. Frontend derives the editable branch suggestion locally from the worktree display name; prompt text and Images are never sent in a worktree request before `task/send`. App Server independently validates the base, branch, repository, destination, and current Git state.

### Removal safeguards

- A linked running Task blocks removal.
- Clicking **Remove worktree** performs a fresh safety preflight. The panel does not rely on a previously displayed clean/dirty result, which would be expensive to maintain and immediately stale.
- An unavailable worktree can be forgotten when no linked Task is running. Because its folder and Git registration are already absent, dirty-file, submodule, lock, and detached-commit checks do not apply.
- Prepared-Task leases do not block confirmed removal. App Server atomically releases and disposes every zero-turn Prepared Task for that worktree, closes their Native Sessions, notifies affected clients, and then proceeds. Each affected Frontend preserves its prompt and Images and falls back to the available Project root; if Project root is unavailable, Send remains blocked until the user chooses another Task Workspace.
- Staged, unstaged, unmerged, and non-ignored untracked files block removal. The user must commit, stash, move, or clean them outside the removal flow.
- The removal preflight explicitly requests all untracked entries and does not let user `status.showUntrackedFiles` or similar presentation configuration hide changes from the safety decision.
- Git-ignored files do not block removal. Confirmation states that ignored and generated content inside the worktree will also be deleted; OpenAIDE does not recursively inventory or size ignored content merely to remove the worktree.
- A clean detached-HEAD worktree with commits not reachable from another branch or tag blocks removal until the commits are preserved.
- When detached commits block removal, OpenAIDE explains that the user can ask the Agent or use external Git to create a branch or tag. V1 adds no branch-management recovery action and never offers forced deletion of those commits.
- A worktree containing initialized submodules cannot be removed through OpenAIDE in v1. The removal surface explains that the user must deinitialize or remove the submodules with external Git tooling first; OpenAIDE does not bypass Git's safeguard with `git worktree remove --force`.
- Removing a worktree never deletes its branch.
- Idle and archived linked Tasks keep their history after removal and become **Workspace unavailable**.
- Removed or forgotten worktrees disappear from Task Workspace selection and management. App Server retains a hidden historical record with the former name, path, and Git reference so linked Tasks remain attributable. If the same path later returns through Git discovery, it receives a new worktree identity and does not silently reconnect old Tasks.
- Confirmation identifies the exact folder, states how many linked Tasks remain readable, and states that the branch is kept.
- A Task with an unavailable workspace cannot continue until the user successfully recreates the recorded path. After **Forget worktree**, linked Tasks remain history-only; continuing work starts a new Task in another Task Workspace.
- Free Prepared Tasks for a removed folder are discarded and their Native Sessions are closed.
