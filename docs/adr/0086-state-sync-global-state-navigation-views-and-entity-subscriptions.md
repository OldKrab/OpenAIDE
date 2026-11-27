# State Sync: Global State, Navigation Views, and Entity Subscriptions

Status: accepted; supersedes parts of [ADR-0023](0023-task-state-publication-and-replica-recovery.md), [ADR-0026](0026-resumable-http-rpc-session.md), [ADR-0029](0029-split-task-metadata-and-chat-storage.md), and [ADR-0073](0073-task-subscription-resumes-from-a-known-revision.md)

App Server synced every kind of state through one mechanism: a publisher sent a scope label and a payload, and both App Server and the client inferred the receiving subscriptions from label, payload kind, and filters, so every routing rule lived twice. Replies and background events reached the wire through different paths, so the client kept per-subscription cursors and a pending-event queue to put them back in order. Permission and Question requests travelled both as server requests and as Task state. A Task subscription doubled as presence, stored twice and kept in step by hand, and it decided archive-older, retention, and request routing. One Task revision covered a 14-field Task patch, so a field that changed without a revision stayed stale behind a "current" resume. Task Navigation was a list of Project windows that native-session discovery republished about 35 times per run, that App Server and client sorted differently, and whose background publisher needed a generation counter to stop an older projection from overwriting a newer one. A Project added after startup got no worktree updates, and a refresh that changed nothing was still published. Measured on one computer, app-wide state other than Navigation is about 14 KB, a Navigation row about 450 bytes, one Project has 924 Codex Native Sessions, and a Task subscription is 118 KB at the median.

State now travels in three shapes, chosen by what the data is like, on a transport whose publish order is its wire order.

## Choice

### Transport

- **Two primitives.** Request and notification, both usable in either direction; App Server accepts client notifications. Every notification method has its own payload type generated from Rust, so a message cannot be sent to the wrong kind of subscriber.
- **Publish order is wire order.** Every outbound message, whether reply, notification, or server request, joins the client session's one sequence when it is produced. There is no separate reply path and background pump, and no per-subscription cursor or client pending-event queue.
- **Recovery.** A link drop is replayed by the reliable session ([ADR-0026](0026-resumable-http-rpc-session.md)). A lost session re-initializes and re-subscribes, which returns full baselines. The recovery barrier stays: product requests wait until the global state and the baselines of every open view and entity are installed. A request whose outcome is unknown is never resent after an App Server restart.
- **Pending requests are Task state only.** Permission and Question requests reach clients through the Task they belong to; their server-request copies are removed.

### Global state

- Projects, worktrees, Agents, settings preferences and runtime settings, New Task defaults, devices, and the terminal list. `client/initialize` returns it in full, and each change is pushed as one `sync` notification. A change that spans areas, such as a worktree's linked count and a Project field, is one notification applied in one step.
- There is no change log and no sync position. A link drop is replayed by the transport; a lost session or an App Server restart re-initializes and receives the full global state again.
- VS Code receives only entries for its Projects. Worktrees are covered for every Project, including one added later, and stay keyed by Worktree Repository because one repository can serve several Projects and carries live linked and running Task counts.
- Settings Skills and Agent details stay requests made by the Settings page; they are most of the settings bytes and only that page shows them. A setting changed elsewhere arrives as a `sync` entry; opening Settings sends nothing.
- Agent recovery state belongs to the Agent entries. Terminal I/O keeps its per-terminal stream ([ADR-0071](0071-terminals-are-shells-on-the-app-servers-computer.md)). Worktrees changed outside OpenAIDE are still found by refresh-on-open; there is no watcher.
- It replaces the Projects, Agents, Settings, Worktree Repository, Devices, and Terminals scopes.

### Task Navigation views

- `navigation/subscribe` takes a section, optional Project ids, a history limit, and an optional open row; `navigation/update` changes them in place. Tasks and Native Sessions are the same kind of row.
- One serialized Navigation owner serves every Project. Its only inputs are "this Project changed", "Projects or Agents changed", and "view parameters changed"; nothing else reads or publishes the projection.
- After the section, enabled-Agent, and Project filters, a row is in a view when it is live (pinned, unread, running, background, stopping, or waiting), among the first history-limit history rows in server order, or the open row. Every Task with a Task Attention Event is live, so attention and shell notifications never depend on the history window.
- App Server owns the order through one comparator. The client never sorts, filters, or limits rows.
- Per view, App Server keeps the parameters and the revision of each row it last sent, and diffs against them on every recompute.
- `navigation/frame` is both the subscribe baseline and every later change. For each changed Project group it carries the complete ordered row ids, bodies only for new or changed rows, the total, hidden, and undiscovered counts, and the discovery state (settled, loading, or failed), plus the group order when it changes. The client rebuilds the group from the order, taking each row from the frame or from its previous copy. There are no insert, remove, or move operations.
- One recompute emits the frames of every affected view together, so a row moving between sections, or a Native Session replaced by the Task that adopted it, never appears twice or goes missing between frames.
- A mutation commits, marks its Project changed, and the resulting frames are sent before the mutation's reply.
- Collapsing a Project hides its non-live rows locally at once, using each row's live flag, and then lowers the history limit.
- **Discovery depth is a count target.** Each Agent listing is paged until it has supplied enough rows to fill the largest history limit among current views, or is exhausted. A catalog page only marks its Project changed, and a page that changes no view sends nothing. The Archive view never drives discovery. Discovery keeps polling, and `taskNavigation/refresh` keeps its name as the explicit request for a new pass; Codex's list-change push is out of scope.
- The open Task comes from its Task subscription and a worktree's linked Tasks from worktree data; no client code assumes Navigation holds every row.

### Entity subscriptions

- A Task, a Subagent history, and a Tool detail are subscribed individually, because they are large and only needed while open. `subscribe` takes the entity and an optional known content version.
- The reply carries every current field in full, and the content (Chat or Tool output) as exactly one of: **current**, when the known version is the latest; **deltas**, the changes after the known version; or **full**, when no version was offered, or it is older than the last compaction or replacement, or ahead of the entity.
- Each committed transaction sends one notification carrying the changed fields and content changes together, so a Task is always atomic.
- Fields carry no version; they are small and always resent on subscribe. Only content is versioned: the Task Chat version is the last Chat frame number, the Tool-detail version its artifact sequence, and a Subagent history keeps its own revision ([ADR-0046](0046-subagent-histories-use-independent-journals.md)). The Subagent catalog is part of the Task entity when it is committed with the Task, and otherwise its own entity subscription. A Subagent history's resume answer is `current` or `full` only: its change journal restarts at every compaction, so deltas would need a change log App Server does not keep, and a history is a bounded tail that is cheap to resend. The Subagent catalog is not committed with the Task (its own revision, written by the Subagent writer), so it stays its own entity subscription, whose reply is always complete.
- The Chat version is durable, so a cached Task copy may resume across an App Server restart. The client drops every cached copy when `client/initialize` reports a different state root; that one check at initialize replaces any per-event state-root comparison.
- A removed entity ends its subscription with a final `removed` notification.
- Agent-owned Composer state (model, mode, Configuration Options, slash commands) is a Task field. Draft text never leaves the client ([ADR-0022](0022-backend-frontend-app-shell-architecture.md)).
- Derived Task fields, such as Task Workspace availability and the Subagent overview, are published to open Tasks when their source changes, since they do not move with a Task commit.
- The client keeps entity copies reference-counted with its existing bounded cache of recent Task copies, so returning to a Task offers its known version.

### Chat changes

- The operations are append an item, upsert an item, append text to a message, and replace the Chat.
- Replace comes only from an authoritative native reload: opening a session that changed elsewhere, or explicit Reload. Agent replay may differ widely from stored history, so it is not diffed. Task creation emits no replacement; the subscribe baseline carries the first state. The fallback that turned an unexplained history change into a replacement is removed, and any mutator it covered records its own changes.
- The client holds a tail window of Chat and loads older pages by request. Changes to messages outside its window are ignored.

### Search

`search` takes text and optional Project ids and returns matching rows from both the open and the archived sections. Each result names its section, the matching message id, and at most one best snippet whose window and highlight ranges App Server computes; a title-only match has no snippet. It is one-shot, not live. While open, its results replace the Navigation list; the Navigation view is untouched. Selecting a result opens the Task at that message. OpenAIDE Tasks match on full text, Native Sessions on title only. The request is `taskNavigation/search`. Matching is a case-insensitive literal phrase over titles and over the User messages and Agent answers of a Task's Chat; the snippet comes from the first matching message in Chat order. Rows with a matching title rank before rows matching only in Chat text, each group newest activity first. App Server bounds a request by result count and by bytes of Chat text scanned, and says when a bound cut the result short. Highlight ranges count UTF-16 code units.

### Chat storage

- Each Task has one `chat.log`: a snapshot record followed by delta frames. Frame numbers never restart; the last one is the Chat version. One Chat-changing transaction is one frame.
- Compaction writes a new file with one snapshot record covering frames up to N and renames it over the old one. A crash before the rename leaves an orphaned temporary file; after it, numbering continues. This covers the crash case that Chat generations exist for, so the per-generation snapshot and journal files and the generation pointer in `task.json` are removed.
- The startup checkpoint in `task.json` is a frame number. A stale checkpoint only triggers a one-time scan of that Task.
- Changes after a version are read from the frames after it and projected per message. The in-memory Task change log is removed.
- The storage version increases, with a migration from the current layout, under ADR-0029's schema rules.

### No presence on the server

A subscription only means "send me this data". Each former use of presence has its own rule:

- **Retention.** Opening a Task refreshes its usage marker; a subscription is not checked.
- **Archive-older.** A Task open in a client has no protection; its open views receive the lifecycle change.
- **Answering a Permission or Question** is an authorization rule: any authenticated client may answer, and a VS Code client only for its Projects' Tasks. Paired Remote Devices keep their current rights. A request can therefore be answered without opening the Task.
- **`secret/read`** goes to a client that holds the key. A client declares the key ids it holds in `client/initialize`, and with a notification when they change.
- Pending requests are not resent on subscribe; they are Task state.
- `task/open` stays the explicit one-shot open ([ADR-0074](0074-task-open-is-an-acknowledgement.md)). Re-subscribing, including after a reconnect, has no side effects, so a client may keep a subscription warm without changing product state.

### Owners and routing

Each shape has one owner: the global-state owner, the Navigation owner, and the existing per-Task commit path for entities. An owner publishes by comparing old and new values, so a change that changes nothing publishes nothing. Moving every existing handler-side publish call behind an owner at once is not required; each owner absorbs them as it lands. Each notification is addressed to known subscriptions; scope labels, payload matching, and state-root comparison on events are removed from both sides. The client session keeps the global state, Navigation views, and entity copies and applies the three message shapes.

Out of scope: the Android attention long-poll ([ADR-0027](0027-task-attention-and-shell-local-notifications.md)), terminal I/O streams, Project file watching, turning `shell/*` server requests into notifications, generic typed change entries, and a single Frontend store.

## Supersedes

- **ADR-0023:** scope-local revisions with revision-gap baselines and per-scope ordered streams; Navigation summary changes published from Task transactions; the Native Session Catalog's depth high-water marks, `taskNavigation/loadMore`, and trusted-descending cutoff, which the code never implemented; cursor-gap replica recovery. The recovery barrier and Native Session Synchronization stand.
- **ADR-0026:** the client-to-server notifications it allowed are now accepted, app events become typed notification methods instead of one event method, and every outbound message joins one session sequence. Sessions, links, and recovery stand.
- **ADR-0029:** Chat generations and the byte-length checkpoint give way to one `chat.log` with monotonic frame numbers and a frame-number checkpoint. Metadata and Chat authority, schema evolution, compaction thresholds, and usage markers stand.
- **ADR-0073:** resume offers a content version rather than a Task revision; fields are always resent; missed changes come from Chat frames rather than an in-memory change log; resume is allowed across App Server restarts; its presence rule is removed. The current, deltas, and full outcomes stand.

## Considered Options

- **A global change log with an ordered sync id and a bounded in-memory buffer.** Rejected. The transport already replays a link drop, and a lost session or restart would empty the buffer anyway, so it would save resending about 14 KB only in a case that is already rare.
- **Every Navigation row to every client.** Rejected: one Project has 924 Native Sessions.
- **Insert, remove, and move operations for Navigation.** Rejected. The complete order is a few kilobytes per changed group and cannot drift from App Server.
- **A timestamp cutoff for discovery.** Rejected. ACP only recommends newest-first listing, the Claude adapter pages by offset, and the Codex adapter sorts by a time it does not report, so a cutoff would stop early.
- **Keep Chat generations.** Rejected. Monotonic frame numbers and atomic rename already tell a committed frame from an obsolete pre-compaction tail.
- **Keep presence in one store.** Rejected. Each former use needs a different rule, and routing `secret/read` to Task subscribers fails when the key is on another device.

## Consequences

- The protocol changes are incompatible with older clients, so the App Server Protocol major version increases ([ADR-0062](0062-remote-devices-are-app-server-clients.md)).
- A Task left open without being opened again, prompted, or worked on for seven days becomes eligible for retention unless it is pinned or otherwise protected; its open view receives the entity `removed` notification, as for any deletion.
- Archive-older can archive a Task another client is viewing; that view shows the change.
- A Permission can be answered without opening its Task, for example from a notification action.
- [The Task Lifecycle and Chat Specification](../task-chat-flow.md) is amended for Navigation, archive-older, retention, search, and Task resume.
