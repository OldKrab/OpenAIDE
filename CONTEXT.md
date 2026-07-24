# OpenAIDE

OpenAIDE is an agent workbench for managing tasks, chat, permissions, settings, and runtime integrations across app shells. This context records project language only, not implementation decisions.

## Language

**OpenAIDE**:
The canonical product identity.
_Avoid_: Alternate product names

**Task**:
A visible unit of agent work in OpenAIDE's task list that has status, Project Context, Agent selection, and one Agent-owned Native Session binding. The binding is immutable after Agent history begins; if an empty New Task session is confirmed missing during preparation or its accepted first prompt, App Server may silently replace that binding before Agent history exists. A Task becomes visible through a durably accepted user message or successful Native Session adoption.
_Avoid_: Chat, conversation

**Durable Task Metadata**:
The Task facts that remain authoritative independently of Chat, including title, Archive state, Project Context, Agent and Native Session binding, and explicit user preferences.
_Avoid_: Live Agent catalogs, transient turn state, treating all Task presentation state as durable metadata

**Transient Task Runtime State**:
Process-owned Task state that is valid only while its App Server and Native Session are live, including Agent command and Configuration Option catalogs, pending requests, and active runtime controls.
_Avoid_: Restoring transient controls as durable Task truth after restart

**Task Activity Time**:
The latest known work or conversation activity for a Task, advanced monotonically by OpenAIDE activity or a newer timestamp from its bound Native Session.
_Avoid_: Task record update time, moving activity backward

**Unknown Activity**:
A Navigation Entry whose source provides no valid activity timestamp. Unknown Activity sorts after every entry with known activity and uses Agent Identity plus Native Session identity as a stable tie-breaker. Discovery time is not activity.
_Avoid_: treating first observation or refresh time as user activity

**New Task**:
The pre-history work surface where a user selects Task context, configures an Agent, and composes the first message. It is backed by an exclusively leased Prepared Task but is not visible in Task Navigation, Task lists, or Archive.
_Avoid_: Draft Task, slot, visible empty Task

**Prepared Task**:
A durable zero-message New Task with its own Agent-owned Native Session. It may be free for reuse or exclusively leased to one client and becomes a visible Task when its first user message is durably accepted.
_Avoid_: Bare Native Session, temporary Frontend-only task

**Prepared-Task Lease**:
Exclusive use of one Prepared Task by one client while that client's New Task composer uses its context and Native Session.
_Avoid_: Shared session, draft ownership map

**Free Prepared Task**:
An unleased, ready Prepared Task eligible for reuse by a client selecting the same Agent and Task Workspace.
_Avoid_: Orphan Task, idle visible Task

**Image**:
Visual content added to a message through paste, drag and drop, or the image picker; those input methods do not create different content kinds.
_Avoid_: PastedImage, treating Image as a workspace-file attachment

**File Attachment**:
A general file explicitly linked to one unsent message through **Attach files**. It is distinct from an Image and from an `@file` mention.
_Avoid_: Upload when the App Shell references an original local path, treating `@file` text as attached content

**Chat**:
The user-facing message surface inside a Task where the user and agent exchange messages and folded tool activity.
_Avoid_: Log-style names

**Archive**:
The read-only lifecycle for Tasks the user no longer needs in ordinary work. Archived Tasks retain saved history, do not interact with their Agent Native Session, and can return to Open only through Restore.
_Avoid_: Treating Archive as a peer task mode, location-only flag, Recent, inactive

**Task Navigation**:
The compact App Shell navigation surface for finding, creating, selecting, archiving, and checking status of Tasks.
_Avoid_: Full Task pages or Settings inside the sidebar

**Project Navigation**:
The Web App and Desktop App navigation surface for switching Projects and seeing Tasks grouped by Project Context.
_Avoid_: Using IDE workspace navigation as the global app model

**Project Context**:
The Project associated with a Task.
_Avoid_: Treating workspace as the Task owner

**Task Workspace**:
The filesystem work area in which a Task's Agent operates. It may be the Project's checkout or a dedicated worktree while the Task remains associated with the same Project Context.
_Avoid_: Treating every Task worktree as a separate Project

**Project root**:
The selected Project's configured filesystem root when used directly as a Task Workspace. It is a stable product choice, not the browser, shell process, or editor's ambient current directory.
_Avoid_: Current folder

**Worktree Repository**:
The Git repository that owns a shared inventory of registered worktrees. Worktree support applies only when the Project folder itself is a Git top-level checkout or linked-worktree root.
_Avoid_: Walking up from a Project subdirectory to infer worktree support

**Unavailable Worktree**:
A worktree Git still registers but whose filesystem work area cannot currently be used. It is inventory state, not a failed Task or a special failed-creation record.
_Avoid_: Failed Worktree

**Managed Worktree**:
A worktree stored under OpenAIDE-managed storage, with a durable OpenAIDE identity independent of its branch or display label.
_Avoid_: Inferring management from which tool last created it

**External Worktree**:
A registered worktree located outside OpenAIDE-managed storage, even when OpenAIDE explicitly recreates it at its recorded path.
_Avoid_: Treating every worktree OpenAIDE can operate on as Managed

**Project**:
A lightweight OpenAIDE record for a user work area, such as a folder, workspace, or repository.
_Avoid_: Git remote or shell-specific workspace identity as the primary identity

**Task Page**:
The main user-facing page for one Task's Chat, composer, permissions, and folded activity.
_Avoid_: Squeezing the main work surface into Task Navigation

**Settings**:
The user-facing configuration area with tabs for Agents, MCP Servers, Skills, and Common Settings.
_Avoid_: Hiding Agent, MCP, or Skill setup inside task-specific controls

**Support Export**:
A hidden command that writes redacted troubleshooting data for bug reports.
_Avoid_: Visible troubleshooting area in first-iteration Settings

**MCP Server**:
A settings-managed tool or resource server configuration that compatible Agents can receive for Task work.
_Avoid_: Per-message hidden tool setup

**Skill**:
A settings-managed local instruction package that OpenAIDE can discover and validate.
_Avoid_: Automatically injected prompt content in the first iteration

**Agent**:
A user-selectable external ACP worker that performs Task work.
_Avoid_: Adapter, provider, raw runtime process, non-ACP protocol selector

**Agent Identity**:
The stable App Server-owned identity for one Agent definition and launch configuration.
_Avoid_: Display name or executable path as identity

**Built-in Agent**:
An Agent option OpenAIDE presents out of the box.
_Avoid_: Editable templates or forcing every common Agent through manual custom setup

**Custom Agent**:
An Agent configured by the user when it is not one of OpenAIDE's built-in Agent options.
_Avoid_: Treating custom configuration as the primary path for common Agents

**Connected Agent**:
An Agent that has successfully launched and initialized, supports the required OpenAIDE ACP capabilities, and has not reported a setup or authentication blocker. A later Agent operation may reveal expired or newly required authentication.
_Avoid_: Letting users start work with an unavailable Agent

**Agent Status**:
The user-facing availability state of an Agent: disconnected, launching, connected, setup required, auth required, authenticating, unsupported, or failed.
_Avoid_: Representing Agent setup problems as failed Tasks

**Setup Required Agent**:
An enabled Agent that cannot begin launch or ACP initialization until the user completes an external prerequisite.
_Avoid_: Treating automatic Agent bootstrap as setup, failed Task

**Failed Agent**:
An Agent whose launch or ACP initialization was attempted and ended unsuccessfully without a known unmet external prerequisite.
_Avoid_: Setup Required Agent, failed Task

**Authentication Method**:
An Agent-advertised way for a user to establish the credentials required for Agent work.
_Avoid_: Provider-specific hard-coded login action, treating an available method as proof that authentication is required

**Authentication Required Agent**:
An initialized Agent that refused an authentication-gated operation until the user completes one of its advertised Authentication Methods.
_Avoid_: Inferring authentication state from available Authentication Methods, starting authentication without user choice

**Authenticating Agent**:
An Agent for which the App Server is running one explicit user-selected Authentication Method.
_Avoid_: Treating available Authentication Methods as active authentication, allowing concurrent auth flows for one Agent

**App Shell**:
The application form OpenAIDE runs in, such as the Web App, Desktop App, Mobile App, or VS Code Extension.
_Avoid_: Host as a product term

**Unattended App Shell**:
An App Shell that is open but does not currently have user focus. When one App Shell has multiple views, it is unattended only when none has focus; Task-level navigation within a focused App Shell does not make it unattended.
_Avoid_: Hidden Task, inactive Task Page

**App Server**:
A local server process that owns product state, task lifecycle, runtime integrations, capability decisions, persistence access, and the protocol used by OpenAIDE App Shells.
_Avoid_: Putting product workflow decisions in Frontend state

**App Server Protocol**:
The bidirectional interface between App Server, Frontend, and App Shells for user intent, product state, events, and user-facing requests.
_Avoid_: App Shell-specific product protocols

**Frontend**:
The part of OpenAIDE that renders product state and captures user intent.
_Avoid_: Owning task lifecycle, settings truth, runtime routing, or stale response rules

**Native Session**:
An Agent-owned session identity that OpenAIDE binds to a Task for live work and Chat loading.
_Avoid_: User-facing replacement for Task, treating OpenAIDE's local Chat projection as the Agent's source of truth

**Native Session Discovery**:
The optional listing of Agent-owned sessions that a user may adopt as OpenAIDE Tasks. Its availability does not determine whether saved OpenAIDE Tasks loaded successfully.
_Avoid_: Agent session history as a synonym for Task history, replacing Task Navigation errors with Agent errors

**Native Session Catalog**:
OpenAIDE's persisted view of Native Sessions last observed through Native Session Discovery. It reflects listing results only; live metadata for an owned Native Session belongs to its active Task runtime. Discovery is driven by demand for one Project and uses working-directory-filtered listing for the Project root and every available worktree in its Worktree Repository, including worktrees not yet referenced by a Task; OpenAIDE does not perform unfiltered, all-Project discovery. Discovery for a Project covers all enabled Agents that support session listing, with independent Agent and Task Workspace requests running in parallel and coalesced by Agent Identity plus canonical Task Workspace root. Each Agent's results are committed and published independently; the Project list is derived by merging the latest committed Agent catalogs rather than waiting at a Project-wide barrier. Discovery is bounded: Navigation is activity-sorted among known entries, but is not guaranteed to contain the globally newest entries unless every Agent is enumerated completely. Discovery optimistically treats each Agent's pages as newest-first and continues an Agent only while its observed activity-time frontier could beat the requested Project cutoff. This is an optimization rather than an ACP guarantee. The Catalog validates the raw Agent order before normalizing it; a missing or invalid activity timestamp, a page that is not descending, or a later page crossing the prior page's activity frontier makes that Agent's ordering untrusted for the refresh. Equal timestamps and duplicate entries do not. An untrusted Agent is discovered up to the requested Project row depth for that Agent, or until exhaustion. As a temporary safety rule, a page that contributes no session identity not already seen in the same live cursor generation stops that Agent and Task Workspace generation even when it returns a next cursor. Whether an identity already exists in the persisted Catalog is irrelevant to this rule. A failed refresh preserves that Agent's last committed entries because failure provides no evidence that they disappeared. Persisted entries survive App Server restarts, but listing cursors and discovery-demand high-water marks do not; each process starts with the current startup demand.
Disabling or removing an Agent hides both its durable Tasks and its unadopted catalog entries from Navigation without deleting them. Re-enabling the same Agent Identity makes retained rows eligible again and starts discovery refresh. A direct URL may still open retained durable history read-only.
_Avoid_: Task store, merging live session updates into listing observations, unfiltered all-Project discovery, discovering only the currently selected Agent, serializing independent Agent or Task Workspace requests, delaying successful Agent results behind another Agent, treating newest-first ordering as an ACP guarantee, claiming bounded discovery is a globally exact newest-first result, clearing entries when refresh fails, deleting entries merely because an Agent is disabled, treating historical load-more depth as permanent startup demand

**Native Session Opening**:
The pre-Task attempt to load a discovered Native Session for adoption. It is identified by its Agent Identity and Native Session identity; success creates a Task, while failure does not.
_Avoid_: Task opening, allocating a Task before adoption succeeds

**Configuration Option**:
An Agent-provided setting for a Native Session whose available choices and current value can change as the Agent state changes.
_Avoid_: Static hard-coded model or mode controls

**Running Task**:
A Task whose primary Agent prompt is active, owned by one App Server process and its Agent Native Session, and which may still need permissions, files, terminals, or user input.
_Avoid_: Blocking all existing Tasks as if they were running

**Task Attention Event**:
A Task state change the user should notice: a turn finished normally, work is waiting for a response, the Agent stopped because it could not continue, or work failed unexpectedly. A user-initiated Stop is not a Task Attention Event.
_Avoid_: Treating every unread update or status change as an alert

## Relationships

- **OpenAIDE** presents agent work as **Tasks**, **Chat**, permissions, and settings through an **App Shell**.
- **App Server** owns product state and workflow decisions.
- **App Server Protocol** carries product state and user intent between **App Server**, **Frontend**, and **App Shells**.
- **App Server Protocol** is transport-neutral.
- **Frontend** renders product state and sends user intent back to **App Server**.
- **Frontend** is shared across App Shells, while App Shell-specific presentation is kept narrow.
- Web App, Desktop App, Mobile App, and VS Code Extension are **App Shells** that connect to an **App Server** for the selected OpenAIDE state root.
- **App Shells** embed or mount **Frontend** through a narrow shell API.
- Web App, Desktop App, and Mobile App share as much **Frontend** composition as their shell constraints allow.
- VS Code Extension composes the same **Frontend** surfaces into VS Code-specific locations.
- An idle **Open Task** can enter the read-only **Archive** lifecycle and later be restored to Open.
- One client holds at most one **Prepared-Task Lease**, and one **Prepared Task** is leased to at most one client.
- A **Free Prepared Task** may be reused by another client selecting the same Agent and Task Workspace.
- Changing Project Context, Agent, or Task Workspace releases the current **Prepared-Task Lease** while leaving the Frontend-owned composer unchanged.
- Removing a worktree releases every **Prepared-Task Lease** for that Task Workspace while leaving affected Frontend-owned composers unchanged.
- Closing or leaving the **New Task** surface for ordinary navigation retains its **Prepared-Task Lease**.
- The leased **Prepared Task** becomes a visible **Task** when its first user message is durably accepted.
- A **Task** belongs to the OpenAIDE task list and has **Project Context**.
- **Project Context** is always a **Project**.
- A **Task** has one **Task Workspace**.
- Project folders that are top-level checkouts of the same **Worktree Repository** share its worktree inventory and management surface.
- An **Unavailable Worktree** remains visible while Git continues to register it, but it cannot be selected as a **Task Workspace**.
- **Managed Worktrees** and **External Worktrees** may both be selected as Task Workspaces when available.
- Tasks using the same worktree share one durable worktree identity rather than copying its mutable workspace facts into each Task.
- A dedicated worktree used as a **Task Workspace** remains associated with the originating **Project Context**.
- A Project folder below a repository root does not receive worktree creation, selection, or management support; it retains **Project root** as its only Task Workspace.
- A **Task** is created only after the user selects the required start context.
- Web App and Desktop App can show task history across Project Contexts.
- VS Code Extension shows Task Navigation for all Project Contexts in its workspace.
- Web App and Desktop App use **Project Navigation** as their primary navigation.
- VS Code Extension groups **Task Navigation** by Project Context, even for a single-Project workspace.
- A **Task** opens one **Chat** backed by its **Native Session**.
- **Task Navigation** opens **Task Pages**.
- **Settings** contains Agents, MCP Servers, Skills, and Common Settings.
- **Support Export** is available through a hidden command, not visible Settings UI in the first iteration.
- **MCP Servers** are configured in **Settings** and made available to compatible **Agents** for Task work.
- **Skills** are managed in **Settings** and are not automatically injected into Agent prompts in the first iteration.
- A **Task** is handled by one **Agent**.
- An **Agent** can handle multiple **Tasks**.
- An **Agent Identity** owns the Agent side of Task history keys.
- **Built-in Agents** and **Custom Agents** are both **Agents**.
- A user can start Agent work only with a **Connected Agent**.
- **Agent Status** explains whether an Agent can currently start work.
- An **App Shell** gives users access to OpenAIDE in Web, Desktop, or VS Code form.
- An **App Shell** reuses an existing compatible **App Server** for the selected state root when one is reachable; otherwise it can start one.
- **App Server** lifetime follows attached **App Shell** clients for its state root, not individual **Frontend** views.
- A **Task** can be bound to one **Native Session**.
- Live interaction with a **Native Session** is owned by one **App Server** process at a time.
- A **Native Session** can expose **Configuration Options**.
- A **Running Task** is owned by one **App Server** process and its **Native Session** while its primary prompt is active.
- A **Task Attention Event** may alert an **Unattended App Shell** while remaining an in-product indicator in a focused App Shell.
- The App Shell client that started a **Running Task** is only the origin for client-scoped capabilities.
- Subscribed App Shell clients can observe a **Running Task**. Connected App Shell clients that advertised the required response capability can answer Task-scoped requests independently of state subscriptions.
- Closing the last **App Shell** client lets **App Server** shut down gracefully; closing a **Task Page** or losing a **Frontend** view is not cancellation.

## Example dialogue

> **Dev:** "When a user opens OpenAIDE, are they starting an agent immediately?"
> **Domain expert:** "No. A **Task** starts only after the user chooses the required Project Context and Agent. OpenAIDE starts the Agent-owned **Native Session** for that Task, but Agent work starts only when the user sends the first message."

> **Dev:** "What happens if the user leaves New Task before sending?"
> **Domain expert:** "Ordinary navigation retains the client's **Prepared-Task Lease**. Returning reopens the composer with the same leased **Prepared Task**, unless a context change or confirmed client expiry released it."

> **Dev:** "Should old chats appear under Recent?"
> **Domain expert:** "Use **Task**, not chat. Old tasks stay in the default list until the user moves them to the **Archive**."

> **Dev:** "Should the main task page be called a log?"
> **Domain expert:** "No. The user-facing surface is **Chat**; implementation can preserve message history without making it feel like logs."

> **Dev:** "Is Codex an adapter or provider in the UI?"
> **Domain expert:** "No. Users choose an **Agent**. Codex is an Agent option; protocol wiring stays out of user-facing labels."

> **Dev:** "Which Built-in Agents ship out of the box?"
> **Domain expert:** "Codex and OpenCode are **Built-in Agents**. Other common ACP Agents can follow later."

> **Dev:** "Can users edit the Codex launch command directly?"
> **Domain expert:** "No. Codex is a fixed **Built-in Agent**. A different command is a **Custom Agent**."

> **Dev:** "How does a user add another ACP program that OpenAIDE does not ship?"
> **Domain expert:** "They add a **Custom Agent** in **Settings**. It becomes another **Agent** option, while **Built-in Agents** remain fixed."

> **Dev:** "What happens if an Agent is not authenticated?"
> **Domain expert:** "It is not a **Connected Agent**. The user fixes setup or auth in Agent settings before starting work."

> **Dev:** "Should a missing Agent binary create a failed Task?"
> **Domain expert:** "No. It appears in **Agent Status** as setup required or failed."

> **Dev:** "Is VS Code the product shell?"
> **Domain expert:** "No. VS Code is one **App Shell**. OpenAIDE can also run as a Web App or Desktop App."

> **Dev:** "Should users choose a session from the task list?"
> **Domain expert:** "No. Users work with **Tasks**. **Native Session** is the Agent-owned identity behind a Task."

> **Dev:** "Can the UI keep a model selector after another option changes?"
> **Domain expert:** "Only if the Agent still exposes that **Configuration Option**. The Agent owns the current option set."

> **Dev:** "Should settings expose default model options?"
> **Domain expert:** "No. **Configuration Options** belong to the current **Native Session**."

> **Dev:** "Can another App Shell continue an old Task?"
> **Domain expert:** "Yes, if its **Native Session** is not live-owned by another **App Server** process. Local Chat is only OpenAIDE's projection; the **Agent** owns the real session state."

> **Dev:** "Should MCP setup live in the composer?"
> **Domain expert:** "No. MCP Servers are managed in **Settings**. A Task may use resolved settings, but setup is not hidden in task controls."

> **Dev:** "Should Settings show a troubleshooting export section?"
> **Domain expert:** "No. First iteration has hidden **Support Export** only."

> **Dev:** "Should the VS Code sidebar show the full Task page?"
> **Domain expert:** "No. The sidebar is **Task Navigation** only. The **Task Page** opens in an editor tab."

> **Dev:** "Does each Agent have its own MCP picker?"
> **Domain expert:** "Not in the first iteration. Enabled **MCP Servers** are available to compatible **Agents**."

> **Dev:** "If a Skill is enabled, does every Task receive it?"
> **Domain expert:** "No. **Skills** are managed metadata in the first iteration; automatic injection is not part of Task runtime."

> **Dev:** "If Desktop is already running and the VS Code Extension opens OpenAIDE, should it start another server?"
> **Domain expert:** "No, it should reuse the compatible **App Server** for the selected state root when reachable. Starting a new one is a fallback."

> **Dev:** "Does OpenAIDE leave an invisible background server after the last shell closes?"
> **Domain expert:** "No. When no **App Shell** clients remain, **App Server** shuts down gracefully. Active work is interrupted or detached locally unless the user explicitly cancels it."

## Flagged ambiguities

- "chat" was used to mean **Task**; resolved: the canonical term is **Task**.
- The main message surface is **Chat**.
- "recent" was used as a task-list section; resolved: use a default task list plus **Archive**, not Active/Recent/All sections.
- "adapter" and "provider" were used around ACP integration; resolved: the user-facing term is **Agent**.
- Common agent integrations were discussed as manual commands; resolved: Codex is the first **Built-in Agent**, while other built-ins and **Custom Agents** can follow later.
- "any ACP agent" means a user-added **Custom Agent** in **Settings**, not a separate Agent category.
- Agent setup failures were discussed as failed Tasks; resolved: unavailable Agents are handled in Agent settings/status before work starts.
- "VS Code extension" was used as if it were the whole product shell; resolved: use **App Shell** for Web App, Desktop App, Mobile App, and VS Code Extension.
- "session" was used ambiguously between ACP and product UI; resolved: **Native Session** is Agent-owned identity, while **Task** remains user-facing work.
- "New Task" was used to conflate the user-facing composer, a durable zero-message Task, and client ownership; resolved: **New Task** is the pre-history work surface, **Prepared Task** is the durable zero-message Task, and **Prepared-Task Lease** is temporary exclusive client use.
- Paste, drag and drop, and image-picker input were discussed as different attachment kinds; resolved: they all add the same **Image** content kind. **File Attachment** remains a distinct general-file content kind.
- "options" was used as if it meant static controls; resolved: **Configuration Options** are Agent-provided and may appear, disappear, or change after Agent updates.
- "default options" was used for saved agent preferences; resolved: do not cache Configuration Option values in v1.
- Cross-App Shell and cross-App Server blocking applies to live **Native Session** interaction, not to observing persisted Task history or answering Task-scoped requests from a subscribed App Shell client.
- "last client" was ambiguous between a Task subscriber and an App Shell client; resolved: losing Task subscribers does not stop a **Running Task**, but losing all **App Shell** clients lets **App Server** shut down.
- "side panel" was used ambiguously; resolved: **Task Navigation** is the sidebar, while **Task Page** is the main work surface.
- Settings was discussed through MCP task selection; resolved: **Settings** has tabs for Agents, MCP Servers, Skills, and Common Settings.
- Diagnostics was unclear user-facing language; resolved: use hidden **Support Export** for first iteration, not visible Settings troubleshooting UI.
- MCP selection was discussed as per-Agent or per-Task; resolved: enabled **MCP Servers** apply to compatible **Agents** in the first iteration.
- Skills were discussed as a Settings tab; resolved: **Skills** are managed metadata only in the first iteration.
