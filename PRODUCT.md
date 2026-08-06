# Product

## Register

product

## Users

Developers and engineering teams using OpenAIDE as a dedicated desktop workspace on macOS and Windows. They need to start and organize agent work, follow execution, inspect terminal and tool activity, respond to permission requests, manage projects and worktrees, and return to previous task history without losing context.

OpenAIDE may also run in web and VS Code shells. Those shells adapt the same product to their host; they do not define the desktop application's visual identity.

## Product Purpose

OpenAIDE is a modern cross-platform desktop application for managing agent tasks from the first prompt through execution and history. Success means the user can understand what the agent is doing, move confidently between projects and tasks, recover previous work, and notice runtime problems without assembling several developer tools by hand.

The desktop application must use platform capabilities where they improve the experience, including native menus, keyboard shortcuts, file and folder pickers, notifications, window behavior, and secure credential storage. It must not be a web page merely wrapped in a desktop window.

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
- Design for desktop first. Use the space, pointer precision, keyboard, window model, and operating-system capabilities of macOS and Windows intentionally.
- Build one recognizable OpenAIDE product. Desktop owns the visual standard; web and VS Code adapt behavior and tokens where their hosts require it.
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
