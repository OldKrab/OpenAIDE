# Tool Detail Artifacts

OpenAIDE will render expanded tool details by ACP `ToolKind`, using one readable detail layout per kind rather than a generic raw JSON view. Collapsed Chat activity stays summarized; grouped activity expands to compact tool rows; a single-tool activity opens directly to that tool's detail.

Full Agent-sent tool output is part of Task Chat history and must be visible when the user expands that tool, including read content, command output, search/fetch output, and edit diffs. Full detail payloads are stored as per-tool-call artifacts and lazy-loaded on expansion so `messages.jsonl`, task snapshots, and pagination stay compact.

Tool detail inputs should be readable structured fields, not raw ACP envelopes. The artifact boundary is the tool call: Chat messages keep the stable tool identity, kind, status, summary, and artifact references, while the detail artifact keeps the full Agent-sent input/output needed for inspection.

Details use ACP tool kinds as their UI taxonomy: read, edit, delete, move, search, execute, think, fetch, switch mode, and other. Inputs may show workspace-relative paths when useful, but not local absolute paths in the summary contract.

Expanded Chat is an inspectability surface, so OpenAIDE will not automatically redact Agent-sent tool input or output there. This decision is scoped to Task Chat; diagnostic/export behavior is separate and not part of this ADR.

Read outputs are shown as full Agent-sent content when ACP returns content, with syntax highlighting when a language can be inferred. Edit outputs use a unified diff UI first. Search output is schema-flexible: locations and parseable grep-like text can render as structured results, but the full Agent-sent output remains available. Fetch output renders as a readable document, formatted JSON, or typed resource preview when possible, with the full Agent-sent output still accessible.

Think output is shown only when ACP sends visible tool content for the `think` call. OpenAIDE must not invent or expose hidden model reasoning that was not sent as tool output.

Delete and move details prioritize structured action facts such as target path, source path, destination path, status, and permission relationship when available. They still show full Agent-sent output when ACP sends output content.

Switch-mode details render as a compact state transition when source/target mode data is available. They show a normal output section only when ACP sends visible output content.

Other tool details use a generic readable fallback: title, kind, status, key/value input fields, locations when present, and full Agent-sent output. OpenAIDE should not invent a custom renderer for an unknown tool until that behavior is promoted to a known detail type.

Permission requests remain their own Chat items while user action is required, but resolved permission state should be linked back into the related tool detail when the ACP tool call identity is available.

Tool details must support progressive updates while a tool is running, not only final completed artifacts. In Task Chat, the most recently updated incomplete block is the active tail block; it is visually latest and the only block with an animated in-progress icon/state. Older incomplete blocks, if any, render static state.

Every incomplete tool row must still show explicit status. Pending and running tools are visibly marked even when they are not the active tail block; only the active tail incomplete block animates.

Completed tool rows stay visually quiet in compact Chat. Failed, pending, and running states carry visible status; completed details may show subtle completion state when useful.

Tool artifacts keep the latest full detail content plus minimal update metadata rather than every intermediate version. Terminal-like output appends stream chunks into one full output artifact.

Tool artifacts remain readable for completed and archived Tasks. Archiving a Task must not reduce Chat history inspectability.

If a tool artifact is missing or corrupt, the Chat row remains visible with its summary data and the expanded detail reports that tool details are unavailable. Artifact read failures must not fail the whole Task snapshot.

Copy is a general Chat affordance for user messages, agent messages, and expanded tool details. The UI should expose quiet copy actions on hover or focus under the relevant message/detail, including copying command/input, output, diffs, paths, and normal message text.

Copy controls appear on hover or keyboard focus on pointer-based layouts. On touch or narrow layouts, compact copy controls remain visible because hover is unavailable.

File references in tool details should behave like editor-native links. In compact titles, click remains reserved for expand/collapse. Inside expanded content, file names and paths are clickable when the Host can open them, with Ctrl/Cmd-click support for users who expect editor-style navigation. Separate open-file buttons are not the primary affordance.

Chat pagination loads message summaries and artifact references, not full artifact content. Artifact content is fetched only when a specific tool is expanded. If OpenAIDE later adds full task-content search, tool artifacts should be included in that search.

Expanded tool details preserve the original ordering of ACP tool content items. OpenAIDE may style each content item by type, but it must not reorder text, diff, terminal, resource, image, or other content blocks into separate grouped sections.

Readable input fields are derived from `rawInput` when ACP provides it. For outputs, ACP `content` is the primary display source; `rawOutput` is shown only when there is no display content or when it adds distinct information as a secondary raw-output section.

Tool details should show timing metadata such as started time, completion time, update time, and duration when the runtime has captured those fields. Timing display is optional and must not block the first implementation if the data is not yet available.
