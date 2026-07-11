# OpenAIDE

OpenAIDE is an agent workbench for managing tasks, chat, permissions, settings, and runtime integrations across app shells. This context records project language only, not implementation decisions.

## Language

**OpenAIDE**:
The canonical product identity.
_Avoid_: Alternate product names

**Task**:
A unit of agent work in OpenAIDE's task list that has status, Project Context, Agent selection, and an Agent-owned Native Session. A Task begins in the Draft phase and becomes Established when App Server durably accepts its first message.
_Avoid_: Chat, conversation

**Draft Task**:
A Task whose required start context is selected and whose Native Session is being acquired or retained, but whose first user message has not yet been accepted. Leaving its page does not discard it.
_Avoid_: Temporary Frontend-only task, orphan prepared session

**Established Task**:
A Task whose first user message has been durably accepted, whether its current Turn is starting, running, completed, interrupted, or failed.
_Avoid_: Treating Agent startup success as the point where the Task becomes real

**Chat**:
The user-facing message surface inside a Task where the user and agent exchange messages and folded tool activity.
_Avoid_: Log-style names

**Archive**:
A place for tasks intentionally removed from the default task list without deleting their history.
_Avoid_: Recent, inactive

**Task Navigation**:
The compact App Shell navigation surface for finding, creating, selecting, archiving, and checking status of Tasks.
_Avoid_: Full Task pages or Settings inside the sidebar

**Project Navigation**:
The Web App and Desktop App navigation surface for switching Projects and seeing Tasks grouped by Project Context.
_Avoid_: Using IDE workspace navigation as the global app model

**Project Context**:
The Project associated with a Task.
_Avoid_: Treating workspace as the Task owner

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
An Agent that has successfully launched, initialized, completed any required setup, and supports the required OpenAIDE ACP capabilities.
_Avoid_: Letting users start work with an unavailable Agent

**Agent Status**:
The user-facing availability state of an Agent: disconnected, launching, connected, setup required, auth required, unsupported, or failed.
_Avoid_: Representing Agent setup problems as failed Tasks

**App Shell**:
The application form OpenAIDE runs in, such as the Web App, Desktop App, Mobile App, or VS Code Extension.
_Avoid_: Host as a product term

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

**Configuration Option**:
An Agent-provided setting for a Native Session whose available choices and current value can change as the Agent state changes.
_Avoid_: Static hard-coded model or mode controls

**Running Task**:
A Task with an active Agent turn owned by one App Server process and its Agent Native Session that may still need permissions, files, terminals, or user input.
_Avoid_: Blocking all existing Tasks as if they were running

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
- A **Task** can be moved to the **Archive**.
- A **Draft Task** is reused for its Project Context until it is established or explicitly discarded.
- Closing a **Task Page** does not discard its **Draft Task** or close its **Native Session**.
- A **Draft Task** becomes an **Established Task** when App Server durably accepts the first user message and starting Turn.
- A **Task** belongs to the OpenAIDE task list and has **Project Context**.
- **Project Context** is always a **Project**.
- A **Task** is created only after the user selects the required start context.
- Web App and Desktop App can show task history across Project Contexts.
- VS Code Extension defaults Task Navigation to the current Project Context.
- Web App and Desktop App use **Project Navigation** as their primary navigation.
- VS Code Extension uses **Task Navigation** within the current Project Context.
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
- A **Running Task** is owned by one **App Server** process and its **Native Session** while its active turn is running.
- The App Shell client that started a **Running Task** is only the origin for client-scoped capabilities.
- Subscribed App Shell clients can observe a **Running Task**. Connected App Shell clients that advertised the required response capability can answer Task-scoped requests independently of state subscriptions.
- Closing the last **App Shell** client lets **App Server** shut down gracefully; closing a **Task Page** or losing a **Frontend** view is not cancellation.

## Example dialogue

> **Dev:** "When a user opens OpenAIDE, are they starting an agent immediately?"
> **Domain expert:** "No. A **Task** starts only after the user chooses the required Project Context and Agent. OpenAIDE starts the Agent-owned **Native Session** for that Task, but the first Agent turn starts only when the user sends work."

> **Dev:** "What happens if the user leaves New Task before sending?"
> **Domain expert:** "The **Draft Task** remains. Returning reopens the same Task and reuses, resumes, loads, or safely replaces its empty **Native Session** behind the App Server seam."

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
- "options" was used as if it meant static controls; resolved: **Configuration Options** are Agent-provided and may appear, disappear, or change after Agent updates.
- "default options" was used for saved agent preferences; resolved: do not cache Configuration Option values in v1.
- Cross-App Shell and cross-App Server blocking applies to live **Native Session** interaction, not to observing persisted Task history or answering Task-scoped requests from a subscribed App Shell client.
- "last client" was ambiguous between a Task subscriber and an App Shell client; resolved: losing Task subscribers does not stop a **Running Task**, but losing all **App Shell** clients lets **App Server** shut down.
- "side panel" was used ambiguously; resolved: **Task Navigation** is the sidebar, while **Task Page** is the main work surface.
- Settings was discussed through MCP task selection; resolved: **Settings** has tabs for Agents, MCP Servers, Skills, and Common Settings.
- Diagnostics was unclear user-facing language; resolved: use hidden **Support Export** for first iteration, not visible Settings troubleshooting UI.
- MCP selection was discussed as per-Agent or per-Task; resolved: enabled **MCP Servers** apply to compatible **Agents** in the first iteration.
- Skills were discussed as a Settings tab; resolved: **Skills** are managed metadata only in the first iteration.
