# Prepared Task Pool and Worktree Tasks

Status: working design note

Related ticket: [#17, Create Tasks in dedicated Git worktrees](https://github.com/OldKrab/OpenAIDE/issues/17)

This note records decisions reached while grilling the worktree proposal. It is not yet the accepted Task Chat specification. The work is deliberately split so the prepared-Task lifecycle can be designed and implemented before worktree behavior depends on it.

## Delivery split

### 1. Leased prepared-Task pool

Replace the current one-New-Task-per-client model with reusable zero-turn prepared Tasks.

This is the current design focus. Finish its lifecycle, ownership, recovery, protocol, and UX decisions before designing its implementation.

### 2. Worktree Task Workspaces

After the pool is complete, implement worktree discovery, creation, selection, Task binding, and management. Worktree Tasks will reuse the prepared-Task leasing mechanism rather than introduce another provisional-session lifecycle.

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

### Part 1 remaining work

- Update the accepted Task Lifecycle and Chat specification and the project glossary after explicit agreement that this working design is complete.
- Derive the implementation and regression-test plan from that accepted specification; do not begin Part 2 worktree implementation until the pool is complete.

## Part 2: Worktree Task Workspaces

### Task creation UX

- Isolation belongs in the existing **Task start context** row with Project and Agent. It does not belong inside the composer.
- Intended control order is Project, Task Workspace, Agent.
- Task Workspace choices are the current checkout, an existing repository worktree, or a new worktree.
- The composer retains prompt text while Task Workspace preparation changes. The final worktree Native Session exists before the user starts the Task, so its Agent options and slash commands are authoritative before Send.
- Creating a worktree is explicit preparation before Send. The Agent is never started against an empty placeholder folder.
- Worktree Tasks remain grouped under the originating Project Context.
- If the Project is a repository subdirectory, the Task Workspace preserves that repository-relative scope inside the worktree.

### Worktree creation

- Default base revision is the source checkout's committed `HEAD`.
- Uncommitted and untracked source-checkout files are not applied to the new worktree. The UI states this when the source is dirty.
- Branch name is derived from the prompt, editable before creation, and validated with Git branch-name rules.
- The creation form lists existing branches and disables creation for a collision. It revalidates during `git worktree add` to handle races and never silently adopts an existing branch.
- An empty or unusable prompt receives a generated fallback branch name.
- OpenAIDE-created worktrees live under managed app storage, organized by repository and worktree identity.
- OpenAIDE honors repository `.worktreeinclude` patterns. Only Git-ignored files may be copied; paths must remain inside the source and destination roots; tracked files cannot be copied through this mechanism.
- Any failed creation or later preparation step produces an error notification identifying the failed step.
- There is no automatic rollback subsystem and no special persisted failed-worktree state.
- After an error, OpenAIDE refreshes Git discovery. Any worktree Git recognizes appears normally in the worktree list and can be inspected, selected, retried, or removed through ordinary management.

### Existing worktrees and Tasks

- Discover all worktrees returned for the repository, including worktrees created by other tools.
- Label worktrees as OpenAIDE-created or external by merging Git discovery with OpenAIDE metadata.
- Any valid worktree may be selected as a Task Workspace.
- Multiple Tasks may use the same worktree concurrently. Reusing a worktree does not mean those Tasks are isolated from each other, and the UI must expose linked and running Task counts.
- Selecting a worktree that already has Tasks creates or leases a separate zero-turn Prepared Task and Native Session. It never clears or reuses a visible Task's conversation.

### Worktree management UX

- Worktree deletion is not part of creating a New Task.
- The primary entry is **Manage worktrees** in the Project actions menu in Task Navigation.
- A secondary **Manage worktrees** shortcut appears in the New Task Workspace selector.
- Management opens a repository-scoped central surface, not a modal and not a permanent sidebar section.
- The panel lists the primary checkout and every linked worktree with path, branch or detached `HEAD`, Git status, linked Task count, running or leased activity, last use, and OpenAIDE/external ownership.
- The primary checkout is visible context but cannot be removed.
- Worktree actions are: **Use for New Task**, **Open folder** when supported, **View linked Tasks**, **Refresh status**, and **Remove worktree**.
- OpenAIDE may remove both OpenAIDE-created and external worktrees after the same safety preflight. External removal explicitly identifies the path and that another tool created it.
- Worktree management does not initially create standalone worktrees, rename or delete branches, merge, rebase, create pull requests, or automatically clean up after Task Archive.
- Task **Archive** already provides the product's close/history lifecycle. Do not add a second Close state solely for worktrees.

### Removal safeguards

- A linked running Task blocks removal.
- Modified or untracked files block removal. The user must commit, stash, move, or clean them outside the removal flow.
- A clean detached-HEAD worktree with commits not reachable from another branch or tag blocks removal until the commits are preserved.
- Removing a worktree never deletes its branch.
- Idle and archived linked Tasks keep their history after removal and become **Workspace unavailable**.
- A Task with an unavailable workspace cannot continue until the user explicitly recreates its worktree at the recorded path and branch.
- Free Prepared Tasks for a removed folder are discarded and their Native Sessions are closed.

### Questions still to grill for Part 2

- Leased-draft removal: whether the initiating client can release its own lease while another live client's lease blocks removal, or removal may invalidate every live zero-turn lease for that folder.
- Exact branch fallback and prompt-to-branch normalization, including non-Latin prompts and maximum length.
- Exact managed path layout and collision strategy across repositories with the same directory name.
- `.worktreeinclude` symlink, size, permissions, secret-copy, partial-copy, and retry rules.
- Base revision picker scope: local branches, remote branches, tags, and arbitrary commits.
- Worktree recreation semantics for Tasks marked **Workspace unavailable**.
- Git discovery and refresh behavior for prunable or missing worktree registrations.
- Cross-shell behavior for opening folders and returning from Worktree management to a preserved New Task draft.
