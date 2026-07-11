# Custom Agent Settings UI

OpenAIDE will let users add user-managed Custom Agents from Settings instead of requiring direct JSON configuration. The first Settings flow creates global Custom Agents only, keeps Built-in Agents fixed, saves incomplete or failing Custom Agent records so Agent Status can guide recovery, and gives each Custom Agent a generated stable identity while allowing the user-facing label to change.

Custom Agent non-secret metadata is stored in the existing global VS Code `openaide.agents` user setting. This keeps the Settings UI on the same source of truth as manually configured Custom Agents and allows users to inspect or sync non-secret Agent definitions through standard VS Code settings.

Because OpenAIDE has not shipped this Custom Agent settings surface, there is no compatibility migration for earlier hand-written development records in `openaide.agents`. The new Settings-owned schema may replace those records rather than preserving legacy shapes.

The Custom Agent launch UI will accept a single command-line field, parse it with shell-like quoting rules, then normalize it into the existing stdio command and args launch model. OpenAIDE will execute the normalized command directly rather than through the system shell, so shell expansion and platform shell behavior are not part of the product contract. Secret-backed launch environment follows ADR 0003: Settings may persist plain non-sensitive values and secret variable names, while secret values remain in Host secret storage and are supplied to the runtime on demand.

The persisted record stores both the original command-line string and the parsed command/args. The command-line string supports Settings round-tripping, while parsed command/args are the canonical launch input for runtime catalog generation.

Settings blocks saving a Custom Agent when the command line cannot be parsed or normalizes to an empty command. Parse failures are local form validation errors, while launch, auth, and ACP compatibility failures are saved records represented through Agent Status.

Custom Agent create and edit flows use an explicit Save action. Field edits do not auto-save because command and environment changes can trigger validation, Agent restarts, and active-work confirmation.

Switching away from a Custom Agent draft with unsaved edits requires discard confirmation. OpenAIDE does not keep hidden per-Agent drafts in the first Settings flow.

The first Settings UI includes add, edit, and remove controls for Custom Agent environment variable rows. Each row is either a plain value row or a secret-backed row; secret values are write-only in the UI.

After saving or editing a Custom Agent, OpenAIDE automatically probes the Agent and updates Agent Status. Settings also keeps an explicit retry action so users can rerun setup checks after installing binaries, changing external auth state, or fixing environment outside OpenAIDE.

Saved Custom Agents that are enabled but not Connected Agents remain visible in the new Task Agent picker but are disabled for Task start. The picker should expose enough status to send users back to Settings for recovery instead of making the Agent disappear. Agents intentionally disabled in Settings are hidden from the new Task Agent picker until re-enabled.

The Agents tab uses a compact Agent list plus detail pane instead of card-per-Agent layout. The list is for scanning Agent identity and status; the detail pane owns launch settings, authentication, environment, status recovery, and destructive actions for the selected Agent.

The first Agent detail pane scope is status/setup, launch command, environment, and danger actions. ACP capability metadata can remain secondary or omitted from the first redesign rather than competing with setup and edit controls.

Agent detail sections render in one scrollable pane rather than nested tabs. The first redesign keeps Settings navigation shallow and avoids hiding setup requirements behind secondary navigation.

The Add Custom Agent action lives in the Agent list toolbar as a compact primary management action. The create flow should open in the detail pane so users can configure and save without a modal-first workflow.

The first redesigned Agent list does not need search or status filters. OpenAIDE expects a small Agent set initially; filtering can be added when the list grows enough to justify the extra control.

Authentication in the Agent detail pane is status-led setup, not a raw auth-method list. OpenAIDE presents the next required setup action, last safe error summary, and retry/recheck controls first; protocol auth method details may be available as secondary detail but should not dominate the page.

Built-in Agent detail panes keep launch policy fixed. Users may enable or disable a Built-in Agent, complete setup or authentication, retry status checks, and inspect status, but editing command, args, or environment requires creating a Custom Agent.

Deleting a Custom Agent removes it from future Agent selection but does not delete existing Tasks. Old Tasks keep their saved Agent label and history and may open in a disconnected/read-only state if the Agent definition is no longer available.

Deleting a Custom Agent requires confirmation because it removes future Agent selection and deletes associated secret environment values.

Editing a Custom Agent command or environment creates a new Agent Identity for future work because launch-affecting settings are part of identity. If the edit would affect a Running Task, OpenAIDE requires explicit confirmation before restarting anything that could interrupt active work.

Disabling any Agent follows the same active-work rule. If disabling would affect a Running Task, OpenAIDE requires explicit confirmation before interrupting or disconnecting active work.

If a saved Custom Agent launches but does not satisfy OpenAIDE's ACP initialize, version, or capability expectations, Settings keeps the record visible and shows Agent Status as unsupported. Unsupported Custom Agents are blocked from new Tasks until edited or removed.

Settings rejects duplicate Custom Agent labels and duplicate normalized launch commands across all Agents, including Built-in Agents. Custom Agent ids are still generated and stable; duplicate validation is about user-facing clarity and accidental duplicate setup, not identity ownership.

Considered alternatives were direct VS Code JSON-only configuration, extension-global or file-backed metadata storage, workspace-local Custom Agents, requiring a successful probe before save, manually chosen ids, duplicate labels or commands, separate command/args fields, shell execution, future-only edits, deleting or blocking deletion when old Tasks reference the Agent, and treating unsupported ACP behavior as a generic failure. The chosen path optimizes the first user-facing flow for fast setup and recoverable errors while preserving stable Task history, Built-in Agent launch policy, and the current ACP stdio runtime boundary.
