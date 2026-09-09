# Product

## Register

product

## Users

Developers and engineering teams using OpenAIDE to manage agent work, primarily inside VS Code for now. They need to start and organize agent work, follow execution, inspect terminal and tool activity, respond to permission requests, manage projects and worktrees, and return to previous task history without losing context.

OpenAIDE also provides desktop and web shells. Each shell adapts the same product to its host. Desktop on macOS and Windows should provide a useful workflow for inspecting and directing agent work while retaining a clear path to VS Code for substantial manual development.

## Product Purpose

OpenAIDE is an agent workbench for managing tasks from the first prompt through execution and history across VS Code, desktop, and web shells. Success means the user can understand what the agent is doing, move confidently between projects and tasks, recover previous work, and notice runtime problems without losing the context needed to act.

The desktop application must use platform capabilities where they improve the experience, including native menus, keyboard shortcuts, file and folder pickers, notifications, window behavior, and secure credential storage. It must not be a web page merely wrapped in a desktop window.

## Current Priorities And Desktop Scope

VS Code is the current product and feature-planning priority. Its existing editor, project navigation, and development tools are part of the user's workflow. Desktop improvements should address concrete gaps in completing agent work without assuming feature parity with VS Code is the goal.

The initial desktop scope is independent access to the project: browse its directory structure, open and read any project file, search across file contents and open a matching line, and inspect changed files and their diffs. These actions must be available without waiting for an Agent to mention a file. Agent File References are additional entry points into the same reading experience. File access and change review use the current Task Workspace so an isolated worktree is not confused with its Project root.

Users should be able to inspect code and request corrections while preserving their conversation and unsent draft. Project files opens a Task Page destination with a persistent navigator beside the reader on wide screens and list/reader navigation on narrow screens. Back to conversation restores the mounted Chat and unsent draft. Substantial manual coding remains a handoff to VS Code. Browsing, content search, change review, and text/range comments share the existing File Viewer capability lifecycle, as specified in the accepted Project file access integration plan.

Small manual edits and Git actions remain separate product decisions, justified by specific interruptions in that workflow. A full editor, debugger, conflict-resolution interface, or general Git client is outside this initial scope. Add controls where the user needs them and judge their value by the workflow they complete and the complexity they introduce.

## Brand Personality

Calm, capable, precise, and contemporary. OpenAIDE should feel thoughtfully made for daily professional use. It should be visually refined without becoming decorative, technical without becoming austere, and powerful without exposing unnecessary complexity.

## Design Judgment

Design guidance describes failure modes, not forbidden components or aesthetics. A pattern can be appropriate when it serves the workflow and is executed well. OpenAIDE should be cautious of:

- imitating another product, platform, or current design trend without understanding why its choices work there;
- choosing a familiar component automatically instead of comparing it with alternatives for the current task;
- treating consistency, density, minimalism, novelty, or visual restraint as goals independent of usability;
- using visual expression that competes with the user's work rather than clarifying it;
- presenting an interface as finished when its hierarchy, states, spacing, interaction, or platform behavior remains unresolved.

## Design Principles

- Best UI/UX is the primary product constraint. Architecture, protocol, runtime, and shell decisions must preserve immediate feedback, clear progress, recoverable errors, and responsiveness under local Agent or App Server latency.
- Prioritize the VS Code experience for now. Use its editor, navigation, and development capabilities where they support the agent workflow; give desktop and web users deliberate alternatives for the workflows those shells support.
- Build one recognizable OpenAIDE product. Share product behavior and design language while adapting layout, tokens, keyboard interaction, and platform capabilities to each host.
- Make agent work inspectable. Chat messages, folded tool activity, terminal output, permission state, and runtime errors must be visible and attributable when relevant.
- Choose hierarchy, containers, typography, alignment, spacing, and grouping according to the relationship being communicated. A container is valuable when it makes that relationship clearer.
- Match control treatment to meaning. Different roles may need different treatments, while shared treatment is valuable when it communicates a genuine relationship.
- Prefer deliberate simplicity over raw density. Remove unnecessary elements, but give the remaining elements enough space, contrast, and visual character to be understood immediately.
- Keep new tasks responsive. The page renders immediately while backend preparation reports readiness or recoverable setup errors without blocking local interaction.
- Preserve readable history. Passive task open shows saved local state without surprising live recovery.
- Treat every visible state as designed. Default, hover, focus, active, selected, loading, empty, error, disabled, and long-content states all belong to the product experience.

## Visual Quality Gate

A screen is not ready merely because it is aligned, consistent, or functional. Before approval, inspect it at realistic macOS and Windows window sizes and ask:

- Does each component treatment clarify its role and relationship to nearby elements?
- Does the chosen sizing use space well for both short and long realistic values?
- Is hierarchy communicated through a deliberate combination of type, color, spacing, position, shape, and interaction?
- Does the composition feel optically balanced rather than only mathematically aligned?
- Does the design make appropriate use of its current shell and window size?
- Are familiar patterns being used intentionally rather than inherited from a library or reference product?
- Does the result feel considered and complete for the workflow it supports?

No visual pattern passes or fails this gate by name. Approval depends on context, purpose, execution, and evidence from the rendered experience.

## Accessibility & Inclusion

Aim for WCAG 2.2 AA across desktop, web, and VS Code shells. Core workflows require keyboard navigation, visible focus, semantic busy and disabled states, reduced-motion compatibility, scalable text, non-color-only status indicators, and layouts that remain usable at narrow window sizes and high display scaling.
